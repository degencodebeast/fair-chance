"""Reading the trade file: every row is kept, and nothing private comes out.

Every test reads the synthetic fixture. It is a fixture, not replay output.
"""
import collections
import csv
import dataclasses
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from trade_file import read_trades  # noqa: E402

FIXTURE = HERE.parent / "fixtures" / "synthetic-trades-fixture.csv"
FIXTURE_WALLETS = {
    "fixture-wallet-creator",
    "fixture-wallet-funded-a",
    "fixture-wallet-funded-b",
    "fixture-wallet-big-a",
    "fixture-wallet-small-a",
    "fixture-wallet-small-d",
    "fixture-wallet-small-b",
    "fixture-wallet-small-c",
}


def every_value(trades):
    return [str(v) for t in trades for v in dataclasses.asdict(t).values()]


class TradeFileTest(unittest.TestCase):
    def setUp(self):
        self.trades = read_trades(FIXTURE)

    def test_every_row_is_read_in_chain_order(self):
        self.assertEqual(len(self.trades), 14)
        self.assertEqual(
            [t.seconds_from_launch for t in self.trades],
            [0, 2, 2, 2, 2, 45, 70, 95, 300, 610, 1900, 2800, 2900, 3000],
        )

    def test_each_role_keeps_every_one_of_its_rows(self):
        with FIXTURE.open(newline="") as file:
            rows_in = collections.Counter(
                "same-funder" if r["role"].startswith("same-funder-") else r["role"]
                for r in csv.DictReader(file)
            )
        file_role = {
            "creator": "creator",
            "funded directly by the same wallet before launch": "same-funder",
            "bought 1% of supply or more in the first hour": "large",
            "other": "other",
        }
        self.assertEqual(collections.Counter(file_role[t.group] for t in self.trades), rows_in)

    def test_trades_carry_no_wallet_no_signature_and_no_slot(self):
        values = every_value(self.trades)
        for wallet in FIXTURE_WALLETS:
            self.assertFalse(any(wallet in v for v in values), wallet)
        self.assertFalse(any("fixture-signature" in v for v in values))
        fields = {f.name for f in dataclasses.fields(self.trades[0])}
        self.assertFalse(fields & {"wallet", "signature", "slot", "seq"})

    def test_each_wallet_becomes_one_account_named_by_its_group_only(self):
        self.assertEqual(
            [t.account for t in self.trades],
            [
                "creator",
                "same-funder-1",
                "same-funder-2",
                "bought-one-percent-1",
                "bought-one-percent-1",
                "other-1",
                "other-2",
                "other-3",
                "other-4",
                "other-1",
                "other-2",
                "creator",
                "other-4",
                "creator",
            ],
        )

    def test_the_file_group_large_is_never_called_large(self):
        self.assertNotIn("large", " ".join(t.account + t.group for t in self.trades))
        self.assertEqual(
            self.trades[3].group, "bought 1% of supply or more in the first hour"
        )


class ReadErrorTest(unittest.TestCase):
    """A bad cell is named by its data row and column only, never by its value."""

    HEADER = (
        "seq,seconds_from_launch,slot,wallet,role,side,sol_lamports,token_base_units,signature,"
        "venue_sol_reserve_before\n"
    )
    GOOD = "1,0,1000,fixture-wallet-a,creator,buy,5,5,fixture-signature-a,3\n"
    VALUE = "Fixture1Base58StringNotAnAddressAbcdefghijkmnop"

    def error(self, row):
        path = Path(tempfile.mkdtemp()) / "trades.csv"
        path.write_text(self.HEADER + self.GOOD + row)
        with self.assertRaises(ValueError) as caught:
            read_trades(path)
        self.assertNotIn(self.VALUE, str(caught.exception))
        return str(caught.exception)

    def test_a_bad_role_names_the_row_and_the_column(self):
        row = f"2,0,1000,fixture-wallet-b,{self.VALUE},buy,5,5,fixture-signature-b,3\n"
        self.assertEqual(self.error(row), "row 2: the role column holds no known role")

    def test_a_bad_side_names_the_row_and_the_column(self):
        row = f"2,0,1000,fixture-wallet-b,other,{self.VALUE},5,5,fixture-signature-b,3\n"
        self.assertEqual(self.error(row), "row 2: the side column must be buy or sell")

    def test_a_bad_number_names_the_row_and_the_column(self):
        for column in ("seq", "seconds_from_launch", "sol_lamports", "token_base_units", "venue_sol_reserve_before"):
            cells = ["2", "0", "1000", "fixture-wallet-b", "other", "buy", "5", "5", "fixture-signature-b", "3"]
            cells[self.HEADER.strip().split(",").index(column)] = self.VALUE
            self.assertEqual(self.error(",".join(cells) + "\n"), f"row 2: the {column} column is not a whole number")

    def test_a_row_with_the_wrong_number_of_cells_names_the_row(self):
        self.assertEqual(self.error(f"2,0,1000,{self.VALUE}\n"), "row 2: the row does not have 10 cells")
        self.assertEqual(
            self.error(f"2,0,1000,fixture-wallet-b,other,buy,5,5,fixture-signature-b,3,{self.VALUE}\n"),
            "row 2: the row does not have 10 cells",
        )


if __name__ == "__main__":
    unittest.main()
