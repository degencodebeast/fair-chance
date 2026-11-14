"""Tests for the sniper scene analysis, on a small run file from a real sandbox run.

fixtures/run-small.json is a full `sniper-scene` run file with the receipt lists, the per-sell
snapshots and every event except fc_buy and fc_sell removed. The expected values below are the
numbers that run printed: the FROM CHAIN ones are sums of its receipts, the CALCULATED ones are
the plain-pool numbers it reported.

Command: python3 -I -m unittest discover -s analysis/sniper-scene/tests
"""
from __future__ import annotations

import contextlib
import copy
import io
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCENE = HERE.parent


def load(name: str):
    sys.path.insert(0, str(SCENE))
    spec = importlib.util.spec_from_file_location(name, SCENE / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


analyze = load("analyze")
FIXTURE = json.loads((HERE / "fixtures" / "run-small.json").read_text())


def run_copy() -> dict:
    return copy.deepcopy(FIXTURE)


def quiet_main(argv: list[str]) -> int:
    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
        return analyze.main(argv)


def early_sells(run: dict) -> list[dict]:
    return [a for a in run["scene"]["actions"] if a["step"] == "sniper sell in early window"]


class Numbers(unittest.TestCase):
    def setUp(self):
        self.d = analyze.derive(run_copy())

    def test_snipers_asked_and_went_through_from_chain(self):
        self.assertEqual(self.d["snipers"], 10)
        self.assertEqual(self.d["asked_total"], "166316154645690880866469951")
        self.assertEqual(self.d["filled_total"], "6557151938527438284136141")
        self.assertEqual(self.d["near_out_with"], "1164998260598154012035735")

    def test_without_fair_chance_is_calculated_on_a_plain_pool(self):
        self.assertEqual(self.d["near_out_without_calculated"], "24566310606687940593252981")
        self.assertEqual(self.d["price_drop_without_percent"], "32.0157")
        self.assertEqual(self.d["price_drop_with_percent"], "1.6578")

    def test_receipt_replay_equals_the_chain_pool(self):
        self.assertTrue(self.d["replay_matches_chain"])
        self.assertEqual(self.d["replay_problems"], [])

    def test_early_sells_refused_inside_the_window(self):
        self.assertEqual(self.d["early_sells_refused_codes"], ["FC_EARLY_WINDOW"])
        self.assertEqual(self.d["early_sells_blocks_after_launch"], [13])

    def test_snipers_are_numbered_by_buy_order(self):
        bags = [int(s["asked"]) for s in self.d["per_sniper"]]
        self.assertEqual(bags, sorted(bags, reverse=True))

    def test_every_number_has_a_label(self):
        text = "\n".join(analyze.summary_lines(self.d, "abc1234"))
        for line in text.splitlines():
            if " NEAR" in line and line.startswith("- ") and ("Fair Chance" in line):
                self.assertTrue("FROM CHAIN" in line or "CALCULATED" in line, line)



class CreatorSell(unittest.TestCase):
    """The scene's creator sells its whole bag last, so all early supply uses the one allowance."""

    def test_creator_sell_uses_up_the_shared_allowance(self):
        d = analyze.derive(run_copy())
        self.assertIsNotNone(d["creator_sell"])
        self.assertGreater(int(d["creator_sell"]["filled"]), 0)
        # Each marked holder's part rounds down: at most one base unit each (10 snipers + creator).
        self.assertLessEqual(int(d["shared_left_end"]), 11)
        text = "\n".join(analyze.summary_lines(d, "abc1234"))
        self.assertIn("The shared allowance left ends at 0", text)

    def test_a_run_without_the_creator_sell_says_so(self):
        run = run_copy()
        run["scene"]["actions"] = [a for a in run["scene"]["actions"] if a["step"] != "creator sell"]
        d = analyze.derive(run)
        self.assertIsNone(d["creator_sell"])
        self.assertIn("The creator did not sell in this run.", "\n".join(analyze.summary_lines(d, "abc1234")))


class EarlySellMargin(unittest.TestCase):
    """The early sells must land in blocks 0 to N. A run where one lands later proves nothing
    about FC_EARLY_WINDOW, so the analysis refuses it and prints no number."""

    def test_refuses_an_early_sell_after_block_n(self):
        run = run_copy()
        early_sells(run)[4]["block_after_launch"] = 18
        with self.assertRaisesRegex(analyze.SceneRefused, "block 18 after launch"):
            analyze.derive(run)

    def test_accepts_an_early_sell_at_block_n(self):
        run = run_copy()
        early_sells(run)[0]["block_after_launch"] = 17
        analyze.derive(run)

    def test_refuses_an_early_sell_with_no_block(self):
        run = run_copy()
        early_sells(run)[0]["block_after_launch"] = None
        with self.assertRaises(analyze.SceneRefused):
            analyze.derive(run)

    def test_refuses_an_early_sell_with_another_code(self):
        run = run_copy()
        early_sells(run)[2]["tx"]["refusal"]["code"] = "FC_CAP_FULL"
        with self.assertRaisesRegex(analyze.SceneRefused, "FC_EARLY_WINDOW"):
            analyze.derive(run)

    def test_refuses_a_run_with_a_failed_check(self):
        run = run_copy()
        run["scene"]["checks"][0]["pass"] = False
        with self.assertRaisesRegex(analyze.SceneRefused, "check failed"):
            analyze.derive(run)

    def test_refused_run_writes_nothing_and_exits_1(self):
        run = run_copy()
        early_sells(run)[0]["block_after_launch"] = 19
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "run.json"
            path.write_text(json.dumps(run))
            out = Path(tmp) / "out"
            self.assertEqual(quiet_main([str(path), str(out), "--no-png"]), 1)
            self.assertFalse(out.exists() and any(out.iterdir()))


class RepoHead(unittest.TestCase):
    """The summary names the code it ran: the commit, marked -dirty when the tree has changes."""

    def git(self, repo: Path, *args: str) -> None:
        subprocess.run(["git", "-C", str(repo), "-c", "user.name=t", "-c", "user.email=t@t",
                        "-c", "commit.gpgsign=false", *args], check=True, capture_output=True)

    def test_clean_tree_gives_the_commit_and_a_changed_tree_adds_dirty(self):
        fc_common = sys.modules["fc_common"]
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            self.git(repo, "init", "-q")
            (repo / "a.txt").write_text("one\n")
            self.git(repo, "add", "a.txt")
            self.git(repo, "commit", "-q", "-m", "first")
            head = subprocess.run(["git", "-C", tmp, "rev-parse", "--short", "HEAD"],
                                  capture_output=True, text=True, check=True).stdout.strip()
            self.assertEqual(fc_common.repo_head(repo), head)
            (repo / "a.txt").write_text("two\n")
            self.assertEqual(fc_common.repo_head(repo), f"{head}-dirty")
            (repo / "a.txt").write_text("one\n")
            (repo / "new.txt").write_text("new\n")
            self.assertEqual(fc_common.repo_head(repo), f"{head}-dirty")


class Output(unittest.TestCase):
    def test_writes_summary_derived_and_guarded_charts(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "run.json"
            path.write_text(json.dumps(FIXTURE))
            out = Path(tmp) / "out"
            self.assertEqual(quiet_main([str(path), str(out), "--no-png"]), 0)
            names = sorted(p.name for p in out.iterdir())
            self.assertIn("summary.md", names)
            self.assertIn("derived.json", names)
            for stem in ("a_asked_vs_through_light", "a_asked_vs_through_contrast",
                         "b_price_drop_light", "b_price_drop_contrast"):
                self.assertIn(f"{stem}.svg", names)
            self.assertFalse(any(n.endswith(".png") for n in names))
            summary = (out / "summary.md").read_text()
            self.assertNotIn("/Users/", summary)
            self.assertIn("FROM CHAIN", summary)
            self.assertIn("CALCULATED", summary)


if __name__ == "__main__":
    unittest.main()
