"""The replay command: inputs from environment variables, output outside the repository.

Runs the command on the synthetic fixture; its output is a fixture result, not replay output.
"""
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
COMMAND = HERE.parent / "replay.py"
FIXTURE = HERE.parent / "fixtures" / "synthetic-trades-fixture.csv"
REPO = HERE.parent.parent.parent


def run(env_extra, *args):
    env = {k: v for k, v in os.environ.items() if not k.startswith("FC_SMDD_")}
    env.update(env_extra)
    return subprocess.run(
        [sys.executable, "-I", str(COMMAND), *args], env=env, capture_output=True, text=True
    )


INPUTS = {"FC_SMDD_TRADES_FILE": str(FIXTURE)}


class ReplayCommandTest(unittest.TestCase):
    def test_it_writes_the_four_outputs_and_prints_no_wallet_and_no_input_path(self):
        out = Path(tempfile.mkdtemp()) / "out"
        result = run(INPUTS, "--out", str(out))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            sorted(p.name for p in out.iterdir()),
            ["readme-section.md", "replay-price-path.svg", "replay-summary.txt", "replay-who-sold.md"],
        )
        printed = result.stdout + result.stderr
        self.assertNotIn("fixture-wallet", printed)
        self.assertNotIn(str(FIXTURE), printed)

    def test_it_plays_both_runs(self):
        out = Path(tempfile.mkdtemp()) / "out"
        result = run(INPUTS, "--out", str(out))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("not run", (out / "replay-who-sold.md").read_text())

    def test_it_runs_the_guard_on_the_method_file_too(self):
        out = Path(tempfile.mkdtemp()) / "out"
        result = run(INPUTS, "--out", str(out))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("replay: the published figures guard passed on METHOD.md", result.stdout)

    def test_it_reports_the_creator_check_against_the_trade_file(self):
        out = Path(tempfile.mkdtemp()) / "out"
        result = run(INPUTS, "--out", str(out))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("creator check: passed", result.stdout)

    def test_a_creator_share_outside_the_tolerance_stops_the_run_and_writes_nothing(self):
        folder = Path(tempfile.mkdtemp())
        changed = folder / "fixture-with-a-wrong-creator-amount.csv"
        text = FIXTURE.read_text().replace(",creator,buy,2000000000,67062500000000,", ",creator,buy,2000000000,90000000000000,")
        text = text.replace(",creator,sell,2200000000,33531250000000,", ",creator,sell,2200000000,56468750000000,")
        changed.write_text(text)
        out = folder / "out"
        result = run({**INPUTS, "FC_SMDD_TRADES_FILE": str(changed)}, "--out", str(out))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("creator check: missed", result.stderr)
        self.assertFalse(out.exists())

    def test_an_output_folder_inside_the_repository_is_refused(self):
        inside = REPO / "target" / "replay-output-must-not-be-written-here"
        result = run(INPUTS, "--out", str(inside))
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(inside.exists())
        self.assertIn("outside the repository", result.stderr)

    def test_a_missing_variable_is_named(self):
        result = run({}, "--out", tempfile.mkdtemp())
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("FC_SMDD_TRADES_FILE", result.stderr)

    def test_an_unreadable_trade_file_writes_nothing_and_prints_no_path(self):
        out = Path(tempfile.mkdtemp()) / "out"
        missing = Path(tempfile.mkdtemp()) / "no-such-trade-file.csv"
        result = run({"FC_SMDD_TRADES_FILE": str(missing)}, "--out", str(out))
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(out.exists())
        self.assertNotIn(str(missing), result.stderr)


if __name__ == "__main__":
    unittest.main()
