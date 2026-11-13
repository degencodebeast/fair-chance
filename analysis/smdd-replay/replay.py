#!/usr/bin/env python3
"""Replay the SMDD first-hour trades on the Fair Chance pool and write a chart, a table and a summary.

The input comes from one environment variable, never printed:
  FC_SMDD_TRADES_FILE      the trade file (CSV; see METHOD.md). Every row is replayed.

Usage:
  python3 -I analysis/smdd-replay/replay.py --out <folder outside the repository>

The output stays outside the repository until a person checks it for privacy.
"""
from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from creator_share_check import check_creator_share  # noqa: E402
from published_figures_guard import METHOD_FILE, findings_in_files, findings_in_text, visible_text  # noqa: E402
from limits_off_pool import LimitsOffPool  # noqa: E402
from limits_off_replay import play_limits_off  # noqa: E402
from limits_on_replay import InProcessCoinRunner, play_limits_on  # noqa: E402
from replay_report import PRECISION_LEVELS, first_passing, render, write_texts  # noqa: E402
from trade_file import read_trades  # noqa: E402

TRADES_VARIABLE = "FC_SMDD_TRADES_FILE"


def _fail(message: str) -> None:
    print(f"replay: {message}", file=sys.stderr)
    sys.exit(1)


def _inside_a_repository(folder: Path) -> bool:
    return any((parent / ".git").exists() for parent in [folder, *folder.parents])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", required=True, type=Path, help="output folder outside the repository")
    args = parser.parse_args()

    if not os.environ.get(TRADES_VARIABLE):
        _fail(f"set {TRADES_VARIABLE}")
    out = args.out.resolve()
    if _inside_a_repository(out):
        _fail("write the output outside the repository; it enters only after a privacy check")
    trade_file = Path(os.environ[TRADES_VARIABLE])
    try:
        trades = read_trades(trade_file)
    except OSError:
        _fail(f"cannot read the file that {TRADES_VARIABLE} names")
    except ValueError as error:
        _fail(str(error))

    # Both runs use the coin's own rules; s is the creator share its fc_rules reports. Limits off
    # has no Fair Chance rule at all, so no early fee.
    runner = InProcessCoinRunner()
    rules, total_supply = runner.rules()
    s = int(rules["creator_fee_share_bps"])
    limits_off = play_limits_off(trades, LimitsOffPool.from_rules(rules, total_supply, s, early_fee=False))
    creator = check_creator_share(limits_off, trade_file)
    if not creator.passed:
        _fail(creator.sentence() + " Nothing written.")
    print(f"replay: {creator.sentence()}")
    limits_on = play_limits_on(trades, runner)
    # METHOD.md is published too.
    method_findings = findings_in_files([Path(__file__).resolve().parent / METHOD_FILE])
    if method_findings:
        for finding in method_findings:
            print(f"replay: the published figures guard failed at {finding}", file=sys.stderr)
        _fail(f"{METHOD_FILE} did not pass the published figures guard; nothing written")
    print(f"replay: the published figures guard passed on {METHOD_FILE}")
    choice = first_passing(
        PRECISION_LEVELS,
        lambda precision: render(limits_off, limits_on, precision, creator),
        lambda texts: [f for name, text in texts.items() for f in findings_in_text(name, visible_text(name, text))],
    )
    if choice.precision is None:
        for finding in choice.findings:
            print(f"replay: the published figures guard failed at {finding}", file=sys.stderr)
        _fail("no precision level passed the published figures guard; nothing written")
    write_texts(choice.texts, out)
    shown = "shares of each run's own totals" if choice.precision.of_run_totals else "tokens sold as shares of supply only"
    form = choice.precision.creator
    creator = {
        "share": f"{form.decimals} decimals",
        "ratio": "the ratio sentence",
        "counts": "the fill counts sentence",
        "not shown": "not shown",
    }[form.kind]
    print(f"replay: the published figures guard passed at one decimal, {shown}; creator limits-on cells: {creator}")
    print(f"replay: wrote the table, the chart, the summary and the README section to {out}")


if __name__ == "__main__":
    main()
