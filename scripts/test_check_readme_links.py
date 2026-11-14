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


def tx_line(claim, sender="alice.testnet", h=H):
    return f'[tx](https://explorer.testnet.near.org/transactions/{h} "claim: {claim} by {sender}")\n'


def sell_log(fill):
    event = {"standard": "fair_chance", "version": "1.0.0-rc.7", "event": "fc_sell", "data": [{"fill": fill}]}
    return "EVENT_JSON:" + json.dumps(event)


def tx_reply(logs=(), failure=None):
    outcome = {"logs": list(logs), "status": {"Failure": failure} if failure else {"SuccessValue": ""}}
    return {"result": {"status": {"SuccessValue": ""}, "receipts_outcome": [{"outcome": outcome}]}}


def fake_rpc(tx=None, accounts=True, seen=None):
    def rpc(method, params):
        if seen is not None:
            seen.append((method, params.get("sender_account_id")))
        if method == "query":
            return {"result": {}} if accounts else {"error": {}}
        return tx
    return rpc


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
        root = make({"README.md": tx_line("partial_fill")})
        seen = []
        rpc = fake_rpc(tx_reply([sell_log("partial")]), seen=seen)
        self.assertEqual(c.check_testnet(root, [root / "README.md"], rpc), [])
        self.assertIn(("EXPERIMENTAL_tx_status", "alice.testnet"), seen)

    def test_a_partial_fill_claim_fails_on_a_full_fill(self):
        root = make({"README.md": tx_line("partial_fill")})
        out = c.check_testnet(root, [root / "README.md"], fake_rpc(tx_reply([sell_log("full")])))
        self.assertEqual(len(out), 1)
        self.assertIn("full_fill", out[0])

    def test_a_full_fill_claim_matches(self):
        root = make({"README.md": tx_line("full_fill")})
        self.assertEqual(c.check_testnet(root, [root / "README.md"], fake_rpc(tx_reply([sell_log("full")]))), [])

    def test_a_refusal_claim_matches_the_code_in_the_receipt(self):
        root = make({"README.md": tx_line("refusal:FC_CREATOR_CAP")})
        failure = {"ActionError": {"kind": {"FunctionCallError": {"ExecutionError": "Smart contract panicked: FC_CREATOR_CAP"}}}}
        self.assertEqual(c.check_testnet(root, [root / "README.md"], fake_rpc(tx_reply(failure=failure))), [])

    def test_a_refusal_claim_fails_on_another_code_or_on_a_fill(self):
        root = make({"README.md": tx_line("refusal:FC_CREATOR_CAP")})
        failure = {"ActionError": {"kind": {"FunctionCallError": {"ExecutionError": "Smart contract panicked: FC_ONE_YOCTO"}}}}
        self.assertEqual(len(c.check_testnet(root, [root / "README.md"], fake_rpc(tx_reply(failure=failure)))), 1)
        self.assertEqual(len(c.check_testnet(root, [root / "README.md"], fake_rpc(tx_reply([sell_log("full")])))), 1)

    def test_a_launch_claim_matches_an_fc_launch_event(self):
        root = make({"README.md": tx_line("launch", sender="creator.testnet")})
        event = {"standard": "fair_chance_factory", "version": "1.0.0", "event": "fc_launch", "data": [{"coin": "bag.f.testnet"}]}
        reply = tx_reply(["EVENT_JSON:" + json.dumps(event)])
        self.assertEqual(c.check_testnet(root, [root / "README.md"], fake_rpc(reply)), [])

    def test_a_launch_claim_fails_without_an_fc_launch_event(self):
        root = make({"README.md": tx_line("launch", sender="creator.testnet")})
        self.assertEqual(len(c.check_testnet(root, [root / "README.md"], fake_rpc(tx_reply([sell_log("full")])))), 1)

    def test_a_claim_without_a_sender_takes_it_from_the_evidence_files(self):
        record = {"network": "testnet", "creator": "maker.testnet",
                  "launch": {"tx": H}, "buys": [{"tx_hash": "8" * 44, "account": "buyer.testnet"}]}
        root = make({"README.md": f'[a](https://explorer.testnet.near.org/transactions/{H} "claim: launch")\n'
                                  f'[b](https://explorer.testnet.near.org/transactions/{"8" * 44} "claim: full_fill")\n',
                     "evidence/run.json": json.dumps(record)})
        seen = []
        event = {"standard": "fair_chance_factory", "version": "1.0.0", "event": "fc_launch", "data": [{}]}

        def rpc(method, params):
            seen.append(params.get("sender_account_id"))
            if method == "query":
                return {"result": {}}
            log = "EVENT_JSON:" + json.dumps(event) if params["tx_hash"] == H else sell_log("full")
            return tx_reply([log])

        self.assertEqual(c.check_testnet(root, [root / "README.md"], rpc), [])
        self.assertIn("maker.testnet", seen)
        self.assertIn("buyer.testnet", seen)

    def test_a_claim_without_a_sender_and_no_evidence_is_reported(self):
        root = make({"README.md": f'[a](https://explorer.testnet.near.org/transactions/{H} "claim: launch")\n'})
        out = c.check_testnet(root, [root / "README.md"], fake_rpc(tx_reply([])))
        self.assertEqual(len(out), 1)
        self.assertIn("sender", out[0])

    def test_a_tx_link_with_no_claim_is_reported(self):
        root = make({"README.md": f"[tx](https://explorer.testnet.near.org/transactions/{H}) alice.testnet\n"})
        out = c.check_testnet(root, [root / "README.md"], fake_rpc(tx_reply([sell_log("partial")])))
        self.assertEqual(len(out), 1)
        self.assertIn("claim", out[0])

    def test_a_tx_that_does_not_resolve_is_reported(self):
        root = make({"README.md": tx_line("partial_fill")})
        out = c.check_testnet(root, [root / "README.md"], fake_rpc({"error": {"message": "unknown"}}))
        self.assertEqual(len(out), 1)


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
