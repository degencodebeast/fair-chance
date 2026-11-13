"""No wallet or signature from the real trade file is in any file git tracks or would add.

Runs only where the trade file is present (FC_SMDD_TRADES_FILE set); skips elsewhere.
It prints a count, never a value.
"""
import csv
import os
import subprocess
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]


@unittest.skipUnless(
    os.environ.get("FC_SMDD_TRADES_FILE"),
    "the trade file is not on this machine",
)
class RepositoryHoldsNoTradeDataTest(unittest.TestCase):
    def test_no_tracked_file_holds_a_wallet_or_a_signature_from_the_trade_file(self):
        secrets = set()
        with open(os.environ["FC_SMDD_TRADES_FILE"], newline="") as file:
            for row in csv.DictReader(file):
                secrets.update((row["wallet"], row["signature"]))
        listed = subprocess.run(
            ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
            cwd=REPO, capture_output=True, check=True,
        ).stdout.decode().split("\0")
        hits = 0
        for name in filter(None, listed):
            path = REPO / name
            if not path.is_file():
                continue
            text = path.read_bytes().decode("utf-8", errors="ignore")
            hits += sum(1 for secret in secrets if secret and secret in text)
        self.assertEqual(hits, 0, f"{hits} trade file value(s) found in tracked files")


if __name__ == "__main__":
    unittest.main()
