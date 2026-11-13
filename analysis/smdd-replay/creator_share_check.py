"""Checks the limits-off run against the trade file: the creator's sold share of supply.

With no Fair Chance rule, the replay's creator should sell about the share the trade file shows
(7.331% of supply for SMDD). The curve shape makes the replay's share smaller: the pump.fun
curve counts 1.073 billion virtual tokens against a supply of 1 billion. The swap fee is not a
cause: pump.fun charged the same 1% fee. The tolerance is 10% of the file value. A miss means the replay does not reproduce the file, so the command writes nothing.
"""
from __future__ import annotations

import csv
from dataclasses import dataclass
from pathlib import Path

from limits_off_replay import Run
from trade_file import CREATOR_GROUP

FILE_SUPPLY_BASE_UNITS = 10**15
TOLERANCE_OF_FILE_VALUE = 0.10
REASON = (
    "tolerance 10% of the file value; the gap comes from the curve shape: the pump.fun curve "
    "counts 1.073 billion virtual tokens against a supply of 1 billion"
)


@dataclass(frozen=True)
class CreatorCheck:
    replay_percent: float
    file_percent: float

    @property
    def gap_points(self) -> float:
        return self.file_percent - self.replay_percent

    @property
    def passed(self) -> bool:
        return abs(self.gap_points) <= TOLERANCE_OF_FILE_VALUE * self.file_percent

    def sentence(self) -> str:
        result = "passed" if self.passed else "missed"
        return (
            f"creator check: {result}. With no Fair Chance rule the creator sold "
            f"{self.replay_percent:.2f}% of supply; the trade file shows {self.file_percent:.3f}%; "
            f"gap {self.gap_points:.2f} points ({100 * self.gap_points / self.file_percent:.1f}% "
            f"of the file value); {REASON}."
        )


def check_creator_share(limits_off: Run, trade_file: Path) -> CreatorCheck:
    sold = sum(s.tokens for s in limits_off.steps if s.group == CREATOR_GROUP and s.side == "sell")
    with Path(trade_file).open(newline="") as file:
        file_sold = sum(
            int(row["token_base_units"])
            for row in csv.DictReader(file)
            if row["role"] == "creator" and row["side"] == "sell"
        )
    return CreatorCheck(
        replay_percent=100 * sold / limits_off.total_supply,
        file_percent=100 * file_sold / FILE_SUPPLY_BASE_UNITS,
    )
