"""The failure printer of scripts/test-trade-replay.sh: it shows fixture failures in full and
hides the details of a test that reads the real trade file. The log below is made up."""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

PRINTER = Path(__file__).resolve().parents[3] / "scripts" / "trade-replay-failures.py"
LOG = """\
..E.F
======================================================================
ERROR: test_after_block_n (test_limits_on_replay.SandboxEquivalenceTest.test_after_block_n)
----------------------------------------------------------------------
Traceback (most recent call last):
RuntimeError: replay-trades-on-sandbox failed: fixture detail

======================================================================
FAIL: test_no_tracked_file (test_repository_holds_no_trade_data.RepositoryHoldsNoTradeDataTest.test_no_tracked_file)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "/private/real/trade-file.csv", line 3
AssertionError: 1 != 0 : planted-wallet-value

----------------------------------------------------------------------
Ran 5 tests in 0.1s

FAILED (failures=1, errors=1)
"""


class TradeReplayFailuresTest(unittest.TestCase):
    def setUp(self):
        log = Path(tempfile.mkdtemp()) / "log.txt"
        log.write_text(LOG)
        self.out = subprocess.run(
            [sys.executable, "-I", str(PRINTER), str(log)], capture_output=True, text=True, check=True
        ).stdout

    def test_a_fixture_failure_shows_in_full(self):
        self.assertIn("ERROR: test_after_block_n", self.out)
        self.assertIn("RuntimeError: replay-trades-on-sandbox failed: fixture detail", self.out)

    def test_a_real_data_failure_shows_only_its_name_and_error_type(self):
        self.assertIn("FAIL: test_no_tracked_file", self.out)
        self.assertIn("AssertionError (details hidden", self.out)
        self.assertNotIn("planted-wallet-value", self.out)
        self.assertNotIn("/private/real", self.out)
        self.assertNotIn("line 3", self.out)


if __name__ == "__main__":
    unittest.main()
