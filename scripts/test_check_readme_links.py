"""Tests for scripts/check_readme_links.py. No network: the external and RPC readers are faked."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_readme_links as c  # noqa: E402


def make(files):
    root = Path(tempfile.mkdtemp())
    for name, text in files.items():
        p = root / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)
    return root


class Slugs(unittest.TestCase):
    def test_github_slug_rules(self):
        self.assertEqual(c.slug("Try it in 90 seconds"), "try-it-in-90-seconds")
        self.assertEqual(c.slug("What `FC_ONE_YOCTO` means (and why)"), "what-fc_one_yocto-means-and-why")
        self.assertEqual(c.slug("Libraries, templates and AI tools"), "libraries-templates-and-ai-tools")

    def test_repeated_headings_get_a_suffix(self):
        anchors = c.anchors("# A\n## B\n## B\n")
        self.assertEqual(anchors, {"a", "b", "b-1"})

    def test_a_heading_inside_a_code_fence_is_not_an_anchor(self):
        self.assertEqual(c.anchors("# A\n```\n# not\n```\n"), {"a"})


class Files(unittest.TestCase):
    def test_a_good_file_and_anchor_pass(self):
        root = make({"README.md": "# T\n[a](docs/x.md#part) [b](#t)\n", "docs/x.md": "## Part\n"})
        self.assertEqual(c.check_local(root, [root / "README.md"]), [])

    def test_a_missing_file_is_reported(self):
        root = make({"README.md": "[a](docs/none.md)\n"})
        out = c.check_local(root, [root / "README.md"])
        self.assertEqual(len(out), 1)
        self.assertIn("docs/none.md", out[0])

    def test_a_missing_anchor_is_reported(self):
        root = make({"README.md": "# T\n[a](#nope)\n"})
        out = c.check_local(root, [root / "README.md"])
        self.assertEqual(len(out), 1)
        self.assertIn("#nope", out[0])

    def test_links_in_code_are_ignored(self):
        root = make({"README.md": "```\n[a](none.md)\n```\n`[b](none.md)`\n"})
        self.assertEqual(c.check_local(root, [root / "README.md"]), [])

    def test_a_folder_link_passes(self):
        root = make({"README.md": "[a](docs/)\n", "docs/x.md": "x\n"})
        self.assertEqual(c.check_local(root, [root / "README.md"]), [])

    def test_the_contents_list_must_link_every_section(self):
        root = make({"README.md": "# T\n## Contents\n- [One](#one)\n## One\n## Two\n"})
        out = c.check_contents(root / "README.md")
        self.assertEqual(len(out), 1)
        self.assertIn("Two", out[0])
        root = make({"README.md": "# T\n## Contents\n- [One](#one)\n- [Two](#two)\n## One\n## Two\n"})
        self.assertEqual(c.check_contents(root / "README.md"), [])


class External(unittest.TestCase):
    def test_each_link_is_asked_once_and_403_and_429_are_reported_not_retried(self):
        root = make({"README.md": "[a](https://ok.example/a) [b](https://slow.example/b) [c](https://gone.example/c)\n"})
        asked = []

        def fetch(url):
            asked.append(url)
            return {"https://ok.example/a": 200, "https://slow.example/b": 429, "https://gone.example/c": 404}[url]

        problems, notes = c.check_external(root, [root / "README.md"], fetch)
        self.assertEqual(len(asked), 3)
        self.assertEqual(len(problems), 1)
        self.assertIn("404", problems[0])
        self.assertEqual(len(notes), 1)
        self.assertIn("429", notes[0])

    def test_one_url_used_twice_is_asked_once(self):
        root = make({"README.md": "[a](https://ok.example/a) [b](https://ok.example/a)\n"})
        asked = []
        c.check_external(root, [root / "README.md"], lambda u: asked.append(u) or 200)
        self.assertEqual(asked, ["https://ok.example/a"])


H = "9" * 44
COIN = "coin.factory.testnet"
FACTORY = "factory.testnet"
FIXTURES = Path(__file__).resolve().parent / "receipt-fixtures"


def tx_line(claim, sender="alice.testnet", on=COIN, h=H):
    return f'[tx](https://explorer.testnet.near.org/transactions/{h} "claim: {claim} by {sender} on {on}")\n'


def sell_log(fill, standard="fair_chance"):
    event = {"standard": standard, "version": "1.0.0-rc.7", "event": "fc_sell", "data": [{"fill": fill}]}
    return "EVENT_JSON:" + json.dumps(event)


def launch_log(standard="fair_chance_factory"):
    return "EVENT_JSON:" + json.dumps({"standard": standard, "version": "1.0.0", "event": "fc_launch", "data": [{}]})


def tx_reply(logs=(), failure=None, signer="alice.testnet", receiver=COIN, emitter=None):
    outcome = {"executor_id": emitter or receiver, "logs": list(logs),
               "status": {"Failure": failure} if failure else {"SuccessValue": ""}}
    return {"result": {"status": {"SuccessValue": ""},
                       "transaction": {"signer_id": signer, "receiver_id": receiver},
                       "receipts_outcome": [{"outcome": outcome}]}}


def fake_rpc(tx=None, accounts=True, seen=None):
    def rpc(method, params):
        if seen is not None:
            seen.append((method, params.get("sender_account_id")))
        if method == "query":
            return {"result": {}} if accounts else {"error": {}}
        return tx
    return rpc


def check(readme_text, reply, extra=None):
    root = make(dict({"README.md": readme_text}, **(extra or {})))
    return c.check_testnet(root, [root / "README.md"], fake_rpc(reply))


class Testnet(unittest.TestCase):
    def test_accounts_are_looked_up_and_a_missing_one_is_reported(self):
        root = make({"README.md": "coin `good.fc.testnet` and `bad.fc.testnet`\n"})

        def rpc(method, params):
            if params.get("account_id") == "bad.fc.testnet":
                return {"error": {"cause": {"name": "UNKNOWN_ACCOUNT"}}}
            return {"result": {"code_hash": "x"}}

        out = c.check_testnet(root, [root / "README.md"], rpc)
        self.assertEqual(len(out), 1)
        self.assertIn("bad.fc.testnet", out[0])

    def test_a_partial_fill_claim_matches_a_partial_fill_receipt(self):
        self.assertEqual(check(tx_line("partial_fill"), tx_reply([sell_log("partial")])), [])

    def test_a_partial_fill_claim_fails_on_a_full_fill(self):
        out = check(tx_line("partial_fill"), tx_reply([sell_log("full")]))
        self.assertEqual(len(out), 1)
        self.assertIn("full_fill", out[0])

    def test_a_full_fill_claim_matches(self):
        self.assertEqual(check(tx_line("full_fill"), tx_reply([sell_log("full")])), [])

    def test_a_refusal_claim_matches_the_code_in_the_receipt(self):
        failure = {"ActionError": {"kind": {"FunctionCallError": {"ExecutionError": "Smart contract panicked: FC_CREATOR_CAP"}}}}
        self.assertEqual(check(tx_line("refusal:FC_CREATOR_CAP"), tx_reply(failure=failure)), [])

    def test_a_refusal_claim_fails_on_another_code_or_on_a_fill(self):
        failure = {"ActionError": {"kind": {"FunctionCallError": {"ExecutionError": "Smart contract panicked: FC_ONE_YOCTO"}}}}
        self.assertEqual(len(check(tx_line("refusal:FC_CREATOR_CAP"), tx_reply(failure=failure))), 1)
        self.assertEqual(len(check(tx_line("refusal:FC_CREATOR_CAP"), tx_reply([sell_log("full")]))), 1)

    def test_the_signer_must_be_the_claimed_sender(self):
        out = check(tx_line("full_fill", sender="bob.testnet"), tx_reply([sell_log("full")], signer="alice.testnet"))
        self.assertEqual(len(out), 1)
        self.assertIn("signer", out[0])

    def test_the_receiver_must_be_the_claimed_coin(self):
        out = check(tx_line("full_fill", on="other.factory.testnet"), tx_reply([sell_log("full")]))
        self.assertEqual(len(out), 1)
        self.assertIn("receiver", out[0])

    def test_a_sell_event_logged_by_another_account_does_not_count(self):
        out = check(tx_line("full_fill"), tx_reply([sell_log("full")], emitter="other.factory.testnet"))
        self.assertEqual(len(out), 1)

    def test_an_event_that_is_not_a_fair_chance_event_does_not_count(self):
        self.assertEqual(len(check(tx_line("full_fill"), tx_reply([sell_log("full", standard="other")]))), 1)

    def test_a_launch_claim_matches_an_fc_launch_event_of_the_factory(self):
        line = tx_line("launch", sender="creator.testnet", on=FACTORY)
        reply = tx_reply([launch_log()], signer="creator.testnet", receiver=FACTORY)
        self.assertEqual(check(line, reply), [])

    def test_a_launch_claim_fails_for_an_fc_launch_event_of_another_standard(self):
        line = tx_line("launch", sender="creator.testnet", on=FACTORY)
        reply = tx_reply([launch_log("other")], signer="creator.testnet", receiver=FACTORY)
        self.assertEqual(len(check(line, reply)), 1)

    def test_a_launch_claim_fails_without_an_fc_launch_event(self):
        line = tx_line("launch", sender="creator.testnet", on=FACTORY)
        reply = tx_reply([sell_log("full")], signer="creator.testnet", receiver=FACTORY)
        self.assertEqual(len(check(line, reply)), 1)

    def test_a_claim_without_a_sender_or_coin_takes_them_from_the_evidence_files(self):
        record = {"network": "testnet", "creator": "maker.testnet", "coin": COIN,
                  "launch": {"tx": H}, "buys": [{"tx_hash": "8" * 44, "account": "buyer.testnet"}]}
        line = (f'[a](https://explorer.testnet.near.org/transactions/{H} "claim: launch")\n'
                f'[b](https://explorer.testnet.near.org/transactions/{"8" * 44} "claim: full_fill")\n')
        replies = {H: tx_reply([launch_log()], signer="maker.testnet", receiver=FACTORY),
                   "8" * 44: tx_reply([sell_log("full")], signer="buyer.testnet")}
        root = make({"README.md": line, "evidence/run.json": json.dumps(record)})

        def rpc(method, params):
            return {"result": {}} if method == "query" else replies[params["tx_hash"]]

        self.assertEqual(c.check_testnet(root, [root / "README.md"], rpc), [])

    def test_a_claim_without_a_sender_and_no_evidence_is_reported(self):
        line = f'[a](https://explorer.testnet.near.org/transactions/{H} "claim: launch")\n'
        out = check(line, tx_reply([]))
        self.assertEqual(len(out), 1)
        self.assertIn("sender", out[0])

    def test_a_tx_link_with_no_claim_is_reported(self):
        line = f"[tx](https://explorer.testnet.near.org/transactions/{H}) alice.testnet\n"
        out = check(line, tx_reply([sell_log("partial")]))
        self.assertEqual(len(out), 1)
        self.assertIn("claim", out[0])

    def test_a_tx_that_does_not_resolve_is_reported(self):
        self.assertEqual(len(check(tx_line("partial_fill"), {"error": {"message": "unknown"}})), 1)


class RealReceipts(unittest.TestCase):
    """Saved replies of the public testnet RPC (fields the check does not read are dropped)."""

    @staticmethod
    def rpc_for_fixtures():
        replies = {}
        for path in FIXTURES.glob("*.json"):
            reply = json.loads(path.read_text())
            replies[reply["result"]["transaction"]["hash"]] = reply

        def rpc(method, params):
            if method == "query":
                return {"result": {}}
            return replies.get(params["tx_hash"], {"error": {"message": "unknown"}})

        return rpc

    def setUp(self):
        self.rpc = self.rpc_for_fixtures()

    def test_every_claim_in_the_readme_matches_its_saved_receipt(self):
        root = Path(__file__).resolve().parent.parent
        readme = root / "README.md"
        out = c.check_testnet(root, [readme], self.rpc)
        self.assertEqual(out, [])

    def test_the_bag_launch_under_the_cabal_coin_fails(self):
        bag = "8imSWsNDvu6xSJtBdeEZ4x4URWPoYCjuhjf5D93UYHKZ"
        line = f'[a](https://testnet.nearblocks.io/txns/{bag} "claim: launch by bag-creator.fairchance.testnet on factory.fairchance.testnet")\n'
        root = make({"README.md": line.replace("bag-creator", "cabal-creator")})
        out = c.check_testnet(root, [root / "README.md"], self.rpc)
        self.assertTrue(any("signer" in o for o in out), out)

    def test_a_full_fill_by_the_wrong_buyer_fails(self):
        later = "jy6vFrc3QSunB5NprSyc7aNT4exGxsjdwbTX8duwMY8"
        line = f'[a](https://testnet.nearblocks.io/txns/{later} "claim: full_fill by cabal-buyer-01.fairchance.testnet on cabal.factory.fairchance.testnet")\n'
        root = make({"README.md": line})
        out = c.check_testnet(root, [root / "README.md"], self.rpc)
        self.assertTrue(any("signer" in o for o in out), out)

    def test_a_sell_claimed_on_the_wrong_coin_fails(self):
        later = "jy6vFrc3QSunB5NprSyc7aNT4exGxsjdwbTX8duwMY8"
        line = f'[a](https://testnet.nearblocks.io/txns/{later} "claim: full_fill by cabal-later-buyer.fairchance.testnet on bag.factory.fairchance.testnet")\n'
        root = make({"README.md": line})
        out = c.check_testnet(root, [root / "README.md"], self.rpc)
        self.assertTrue(any("receiver" in o for o in out), out)


class LaunchedCoin(unittest.TestCase):
    def test_a_launch_names_the_coin_it_started(self):
        bag = "8imSWsNDvu6xSJtBdeEZ4x4URWPoYCjuhjf5D93UYHKZ"
        rpc = RealReceipts.rpc_for_fixtures()
        for coin, want in (("bag.factory.fairchance.testnet", 0), ("cabal.factory.fairchance.testnet", 1)):
            line = (f'[a](https://testnet.nearblocks.io/txns/{bag} '
                    f'"claim: launch by bag-creator.fairchance.testnet on factory.fairchance.testnet of {coin}")\n')
            root = make({"README.md": line})
            self.assertEqual(len(c.check_testnet(root, [root / "README.md"], rpc)), want, coin)


class Outside(unittest.TestCase):
    def test_a_link_that_leaves_the_repo_is_reported(self):
        base = make({"outside.md": "# x\n", "repo/README.md": "[a](../outside.md)\n"})
        root = base / "repo"
        out = c.check_local(root, [root / "README.md"])
        self.assertEqual(len(out), 1)
        self.assertIn("outside", out[0])

    def test_the_case_of_a_path_must_match_exactly(self):
        root = make({"README.md": "[a](docs/X.md) [b](Docs/x.md)\n", "docs/x.md": "# x\n"})
        self.assertEqual(len(c.check_local(root, [root / "README.md"])), 2)

    def test_an_anchor_is_checked_for_an_upper_case_md_suffix(self):
        root = make({"README.md": "[a](docs/x.MD#nope)\n", "docs/x.MD": "# x\n"})
        self.assertEqual(len(c.check_local(root, [root / "README.md"])), 1)


class Flags(unittest.TestCase):
    def test_an_unknown_flag_fails(self):
        with self.assertRaises(SystemExit) as caught:
            c.parse(["--extrenal"])
        self.assertEqual(caught.exception.code, 2)

    def test_the_known_flags_parse(self):
        args = c.parse(["--external", "--testnet", "README.md"])
        self.assertTrue(args.external and args.testnet)
        self.assertEqual(args.files, ["README.md"])


if __name__ == "__main__":
    unittest.main()
