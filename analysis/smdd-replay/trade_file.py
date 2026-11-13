"""Reads the SMDD first-hour trade file and drops everything private.

Columns in: seq, seconds_from_launch, slot, wallet, role, side, sol_lamports, token_base_units,
signature, venue_sol_reserve_before. Out: one `Trade` per row in chain order, every row kept,
each wallet replaced by an account named for its group. No wallet, signature, slot or seq leaves
this module, and no error message holds a cell value: it names the row and the column. The venue depth goes on to size buys; no output
shows it.
"""
from __future__ import annotations

import csv
from dataclasses import dataclass
from pathlib import Path

ONE_PERCENT_GROUP = "bought 1% of supply or more in the first hour"
OTHER_GROUP = "other"
CREATOR_GROUP = "creator"
SAME_FUNDER_GROUP = "funded directly by the same wallet before launch"

# The file's own role names. Its "large" means bought 1% of supply or more in the file; that word
# is a contract class here, so it never reaches an account or a label.
_FILE_ROLE_TO_GROUP = {"creator": CREATOR_GROUP, "large": ONE_PERCENT_GROUP, "other": OTHER_GROUP}
_ACCOUNT_PREFIX = {ONE_PERCENT_GROUP: "bought-one-percent", OTHER_GROUP: "other"}
_SAME_FUNDER_ROLES = {f"same-funder-{n}" for n in range(1, 6)}
_COLUMNS = [
    "seq",
    "seconds_from_launch",
    "slot",
    "wallet",
    "role",
    "side",
    "sol_lamports",
    "token_base_units",
    "signature",
    "venue_sol_reserve_before",
]


@dataclass(frozen=True)
class Trade:
    account: str
    group: str
    seconds_from_launch: int
    side: str
    sol_lamports: int
    token_base_units: int
    # Private: SOL depth of the trade's own venue just before it. Sizes buys; never shown.
    venue_sol_reserve_before: int


def read_trades(path: Path) -> list[Trade]:
    """Every row as a `Trade`, in chain order. An error names the data row (1 is the first row
    under the header) and the column, never a cell's value."""
    with Path(path).open(newline="") as file:
        reader = csv.DictReader(file)
        if reader.fieldnames != _COLUMNS:
            raise ValueError("the trade file header is not the expected column list")
        rows = [_checked(number, row) for number, row in enumerate(reader, 1)]
    rows.sort(key=lambda numbered: numbered[1]["seq"])
    accounts: dict[str, tuple[str, str]] = {}
    counts: dict[str, int] = {}
    trades = []
    for number, row in rows:
        if row["wallet"] not in accounts:
            accounts[row["wallet"]] = _account_for(number, row["role"], counts)
        account, group = accounts[row["wallet"]]
        trades.append(
            Trade(
                account=account,
                group=group,
                seconds_from_launch=row["seconds_from_launch"],
                side=row["side"],
                sol_lamports=row["sol_lamports"],
                token_base_units=row["token_base_units"],
                venue_sol_reserve_before=row["venue_sol_reserve_before"],
            )
        )
    return trades


_WHOLE_NUMBER_COLUMNS = (
    "seq",
    "seconds_from_launch",
    "sol_lamports",
    "token_base_units",
    "venue_sol_reserve_before",
)


def _checked(number: int, row: dict) -> tuple[int, dict]:
    """The row with its numbers parsed. The message names the row and the column only."""
    if None in row or None in row.values():
        raise ValueError(f"row {number}: the row does not have {len(_COLUMNS)} cells")
    if row["side"] not in ("buy", "sell"):
        raise ValueError(f"row {number}: the side column must be buy or sell")
    parsed = dict(row)
    for column in _WHOLE_NUMBER_COLUMNS:
        try:
            parsed[column] = int(row[column])
        except ValueError:
            raise ValueError(f"row {number}: the {column} column is not a whole number") from None
    return number, parsed


def _account_for(number: int, role: str, counts: dict[str, int]) -> tuple[str, str]:
    """The account name and group of a wallet's first row, numbered in order of first trade."""
    if role in _SAME_FUNDER_ROLES:
        return role, SAME_FUNDER_GROUP
    if role not in _FILE_ROLE_TO_GROUP:
        raise ValueError(f"row {number}: the role column holds no known role")
    group = _FILE_ROLE_TO_GROUP[role]
    if group == CREATOR_GROUP:
        return "creator", group
    counts[group] = counts.get(group, 0) + 1
    return f"{_ACCOUNT_PREFIX[group]}-{counts[group]}", group
