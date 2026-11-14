"""Writes the replay's published outputs: the table, the chart, a one-line summary and the README
section. Every one goes through `published_figures_guard` before anything is written.

Privacy rules the outputs keep:
- groups only, no account names or addresses; minutes from launch, never a clock time;
- sells only, as shares of each run's own totals: tokens sold as a share of all tokens sold in
  that run, NEAR paid out as a share of all NEAR paid out in that run. No NEAR amount, no buy
  total and no trade count is shown. A nonzero share never shows as zero;
- the chart has words only, no numbers;
- a group other than the creator shows its cells only when it has at least MIN_GROUP_SIZE wallets
  and the share comes from at least MIN_GROUP_SIZE sells; else the cell says "Not shown". A run's
  shares add up to 100%, so a column never holds exactly one hidden cell: the smallest other
  group cell (never the creator's) is hidden with it;
- the file group of wallets that bought 1% of supply or more is never called large. "large" in
  the rule engine column is the engine's class.

Precision falls back on its own. The table shows one decimal, as shares of each run's own totals,
then as tokens sold as a share of supply only. No level shows whole percents: the guard checks
every value inside the shown rounding, so coarser rounding fails more often. The creator's
limits-on cells have their own steps: two decimals, then three; then a ratio sentence; then the
creator's fill counts only; last, the cells are not shown, with a line that says why. The summary
and the README repeat those cells in the same form. The first level that passes the guard on
every output is written.
"""
from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

from creator_share_check import CreatorCheck
from limits_off_replay import SIZING, Run
from trade_file import CREATOR_GROUP, ONE_PERCENT_GROUP, OTHER_GROUP, SAME_FUNDER_GROUP

GROUP_ORDER = [CREATOR_GROUP, SAME_FUNDER_GROUP, ONE_PERCENT_GROUP, OTHER_GROUP]
LIMITS_ON_RUNNER = "the same coin code, in the near-sdk in-process runner, with exact blocks and times"
LIMITS_ON_LABEL = "all rules together (early fee, allowance, class and cap)"
LIMITS_OFF_LABEL = "no Fair Chance rule: no early fee and no sell limit, the same pool math with its normal swap fee"
CLASS_ORDER = ["small", "band", "large"]
TABLE_FILE = "replay-who-sold.md"
CHART_FILE = "replay-price-path.svg"
SUMMARY_FILE = "replay-summary.txt"
README_FILE = "readme-section.md"


NOT_SHOWN_LINE = "Not shown: this figure does not pass the privacy guard in any form."
NOT_SHOWN_CELL = "Not shown"
# A group's cell shows only when the group has at least this many wallets and the share comes
# from at least this many sells (a sell that filled none does not count). The builder may raise it.
# The creator is a named role, so its cells always show.
MIN_GROUP_SIZE = 2
GROUP_NOT_SHOWN_LINE = (
    f"Not shown: a group of fewer than {MIN_GROUP_SIZE} wallets, a share from fewer than "
    f"{MIN_GROUP_SIZE} sells, or a share that would give another hidden figure back."
)


@dataclass(frozen=True)
class CreatorForm:
    """How the creator's limits-on cells show: "share" (at `decimals`), "ratio" (the creator's
    limits-on payout as a share of its limits-off payout, at `decimals`), "counts" (fills only) or
    "not shown"."""

    kind: str
    decimals: int = 0


CREATOR_FORMS = [
    CreatorForm("share", decimals=2),
    CreatorForm("share", decimals=3),
    CreatorForm("ratio", decimals=2),
    CreatorForm("counts"),
    CreatorForm("not shown"),
]


@dataclass(frozen=True)
class Precision:
    decimals: int
    of_run_totals: bool  # shares of each run's own totals; else tokens sold as a share of supply
    creator: CreatorForm  # the creator's limits-on cells


PRECISION_LEVELS = [
    Precision(1, of_run_totals=run_totals, creator=form)
    for run_totals in (True, False)
    for form in CREATOR_FORMS
]


def _sold_by_group(run: Run) -> dict[str, tuple[int, int]]:
    """Tokens sold and NEAR paid out per group."""
    sold: dict[str, list[int]] = defaultdict(lambda: [0, 0])
    for step in run.steps:
        if step.side == "sell":
            sold[step.group][0] += step.tokens
            sold[step.group][1] += step.near
    return {group: (tokens, near) for group, (tokens, near) in sold.items()}


def _what_the_engine_did(run: Run, group: str) -> str:
    """Classes, sell limits and refusals the rule engine gave this group's trades, as words only."""
    steps = [step for step in run.steps if step.group == group]
    classes = sorted({s.holder_class for s in steps if s.holder_class}, key=CLASS_ORDER.index)
    fill_words = {"partial": "filled in part", "none": "filled none"}
    held = sorted({fill_words[s.fill] for s in steps if s.fill in fill_words})
    reasons = sorted({f"held back by {s.limit_reason}" for s in steps if s.limit_reason})
    refusals = sorted({f"refused {s.refusal}" for s in steps if s.refusal})
    parts = []
    if classes:
        parts.append("class " + ", ".join(classes))
    parts += held + reasons + refusals
    return "; ".join(parts) or "no trade"


def shown_share(part: int, whole: int, decimals: int) -> str:
    """A share as shown text. A nonzero share never shows as zero: it shows "below 0.1%" (or
    "below 1%" at whole percents). Only a true zero shows "0%"."""
    if part == 0 or whole == 0:
        return "0%"
    shown = f"{100 * part / whole:.{decimals}f}"
    if float(shown) == 0:
        return f"below {10 ** -decimals:.{decimals}f}%"
    return shown + "%"


def _wallets(run: Run, group: str) -> int:
    return len({step.account for step in run.steps if step.group == group})


def _too_small(run: Run, group: str) -> bool:
    """A group other than the creator with fewer than MIN_GROUP_SIZE wallets in this run."""
    return group != CREATOR_GROUP and _wallets(run, group) < MIN_GROUP_SIZE


class _Shares:
    """One run's shares per group, at one precision. A small group's share is not shown."""

    def __init__(self, run: Run, decimals: int) -> None:
        self.run = run
        self.sold = _sold_by_group(run)
        self.tokens_sold = sum(tokens for tokens, _ in self.sold.values())
        self.paid_out = sum(near for _, near in self.sold.values())
        self.supply = run.total_supply
        self.decimals = decimals

    def _group(self, group: str) -> tuple[int, int]:
        return self.sold.get(group, (0, 0))

    def _hidden(self, group: str, figure: str) -> bool:
        """True when a share is too close to one wallet or one sell: a group too small, or a
        nonzero share from fewer than MIN_GROUP_SIZE sells."""
        if group == CREATOR_GROUP:
            return False
        sells = sum(
            1
            for s in self.run.steps
            if s.group == group and s.side == "sell" and getattr(s, figure) > 0
        )
        return _too_small(self.run, group) or 0 < sells < MIN_GROUP_SIZE

    def _share(self, group: str, figure: str, whole: int) -> str:
        if self._hidden(group, figure):
            return NOT_SHOWN_CELL
        part = self._group(group)[0 if figure == "tokens" else 1]
        return shown_share(part, whole, self.decimals)

    def of_tokens_sold(self, group: str) -> str:
        return self._share(group, "tokens", self.tokens_sold)

    def of_payout(self, group: str) -> str:
        return self._share(group, "near", self.paid_out)

    def of_supply(self, group: str) -> str:
        return self._share(group, "tokens", self.supply)


TOKENS_OF_RUN = "Tokens sold, limits {} (share of all tokens sold in that run)"
NEAR_OF_RUN = "NEAR paid out, limits {} (share of all NEAR paid out in that run)"
TOKENS_OF_SUPPLY = "Tokens sold, limits {} (share of supply)"


def _columns(shares: _Shares, side: str, precision: Precision) -> list[tuple[str, Callable[[str], str]]]:
    if precision.of_run_totals:
        return [
            (TOKENS_OF_RUN.format(side), shares.of_tokens_sold),
            (NEAR_OF_RUN.format(side), shares.of_payout),
        ]
    return [(TOKENS_OF_SUPPLY.format(side), shares.of_supply)]


def fill_counts_sentence(sells: int, full: int, part: int, none: int) -> str:
    """The creator's limits-on fills as counts only. A group of zero is not named."""
    groups = [
        f"{count} {words}"
        for count, words in ((full, "filled in full"), (part, "filled in part"), (none, "filled none"))
        if count
    ]
    listed = groups[0] if len(groups) == 1 else ", ".join(groups[:-1]) + " and " + groups[-1]
    return f"With limits on, of the creator's {sells} sells, {listed}."


@dataclass(frozen=True)
class _CreatorOn:
    cells: dict[str, str]  # limits-on column name -> the creator's cell
    line: str | None  # the line under the table, when the cells are not shares
    phrase: str  # what the creator did with the limits on, for the summary and the README


def _creator_on(limits_off: Run, limits_on: Run, precision: Precision) -> _CreatorOn:
    form = precision.creator
    on_columns = [name for name, _ in _columns(_Shares(limits_on, 1), "on", precision)]
    if form.kind == "share":
        shares = _Shares(limits_on, form.decimals)
        cells = {name: cell(CREATOR_GROUP) for name, cell in _columns(shares, "on", precision)}
        return _CreatorOn(cells, None, "the creator " + _creator_sold(shares, precision))
    if form.kind == "ratio":
        payout_off = _Shares(limits_off, 1)._group(CREATOR_GROUP)[1]
        payout_on = _Shares(limits_on, 1)._group(CREATOR_GROUP)[1]
        phrase = (
            f"the creator took out {shown_share(payout_on, payout_off, form.decimals)} of what it "
            "took with limits off"
        )
        line = f"With limits on, {phrase}."
    elif form.kind == "counts":
        sells = [s for s in limits_on.steps if s.group == CREATOR_GROUP and s.side == "sell"]
        full = sum(1 for s in sells if s.fill == "full")
        part = sum(1 for s in sells if s.fill == "partial")
        line = fill_counts_sentence(len(sells), full, part, len(sells) - full - part)
        phrase = line.removeprefix("With limits on, ").removesuffix(".")
    else:
        line = NOT_SHOWN_LINE
        phrase = "the creator's figure is not shown. " + NOT_SHOWN_LINE.removesuffix(".")
    shown = "see the line under the table" if form.kind != "not shown" else NOT_SHOWN_CELL
    return _CreatorOn({name: shown for name in on_columns}, line, phrase)


DIRECT_FUNDING_LINE = (
    "The same-funder check sees direct funding only, so wallets funded through other wallets can "
    "sit in \"other\"."
)


def same_funder_line(limits_off: Run, limits_on: Run, decimals: int) -> str | None:
    """The same-funder group's counts line, from the runs: how many wallets, and their share of all
    NEAR paid in for buys in the limits-off run. Only when the group sold nothing in either run."""
    group = [s for run in (limits_off, limits_on) for s in run.steps if s.group == SAME_FUNDER_GROUP]
    if not group or any(s.side == "sell" for s in group):
        return None
    wallets = len({s.account for s in group})
    buys = [s for s in limits_off.steps if s.side == "buy"]
    if wallets < MIN_GROUP_SIZE or sum(1 for s in buys if s.group == SAME_FUNDER_GROUP) < MIN_GROUP_SIZE:
        return None
    paid = shown_share(
        sum(s.near for s in buys if s.group == SAME_FUNDER_GROUP), sum(s.near for s in buys), decimals
    )
    return (
        f"{wallets} wallets funded directly by one wallet before launch paid {paid} of all NEAR paid "
        "in for buys in the first hour and sold none in the first hour."
    )


def _table_shares(limits_off: Run, limits_on: Run, precision: Precision) -> tuple[list[str], dict[str, list[str]]]:
    """The share column names and each group's shown cells, after the subtraction rule."""
    off, on = _Shares(limits_off, precision.decimals), _Shares(limits_on, precision.decimals)
    columns = _columns(off, "off", precision) + _columns(on, "on", precision)
    creator_on = _creator_on(limits_off, limits_on, precision)
    shares = {
        group: [
            creator_on.cells[name] if group == CREATOR_GROUP and name in creator_on.cells else cell(group)
            for name, cell in columns
        ]
        for group in GROUP_ORDER
    }
    _hide_what_subtraction_gives_back(shares)
    return [name for name, _ in columns], shares


def _counts_line(limits_off: Run, limits_on: Run, precision: Precision) -> str | None:
    """The same-funder counts line, unless a sell cell of the group is hidden: "sold none" would
    give that cell back."""
    _, shares = _table_shares(limits_off, limits_on, precision)
    if NOT_SHOWN_CELL in shares[SAME_FUNDER_GROUP]:
        return None
    return same_funder_line(limits_off, limits_on, precision.decimals)


def table(limits_off: Run, limits_on: Run, precision: Precision) -> str:
    names, shares = _table_shares(limits_off, limits_on, precision)
    creator_on = _creator_on(limits_off, limits_on, precision)
    header = ["Group"] + names + ["What the rule engine did, limits on"]
    lines = [
        "SMDD first hour: replay with the limits off and on.",
        "",
        "| " + " | ".join(header) + " |",
        "|" + " --- |" * len(header),
    ]
    for group in GROUP_ORDER:
        engine = NOT_SHOWN_CELL if _too_small(limits_on, group) else _what_the_engine_did(limits_on, group)
        cells = [group] + shares[group] + [engine]
        lines.append("| " + " | ".join(cells) + " |")
    lines.append("")
    if any(NOT_SHOWN_CELL in line for line in lines[4:] if not line.startswith("| creator |")):
        lines += [GROUP_NOT_SHOWN_LINE, ""]
    if creator_on.line:
        lines += [creator_on.line, ""]
    counts = _counts_line(limits_off, limits_on, precision)
    if counts:
        lines += [counts, ""]
    if precision.of_run_totals:
        lines += [
            "Each share is a share of that run's own total: all tokens sold, or all NEAR paid out "
            "to sellers, with one total for the limits-off run, another for the limits-on run. The "
            "two runs have different totals, so the shares are not amounts and the columns of the "
            "two runs do not compare as tokens or NEAR.",
            "",
        ]
    else:
        lines += [
            "A token can be sold more than once in the hour (bought back, then sold again), so the "
            "shares sold can add up to more than the whole supply.",
            "",
        ]
    lines += [
        f"Limits off: {LIMITS_OFF_LABEL}, in the script. Limits on: {LIMITS_ON_LABEL}, in "
        f"{LIMITS_ON_RUNNER}. The difference between the two runs comes from all these rules "
        "together, not from one of them.",
        "",
        "How the replay sizes a trade: " + SIZING,
        "",
    ]
    return "\n".join(lines)


def _shown_value(cell: str) -> float | None:
    """A shown share as a number ("below 0.1%" counts as 0), or None when the cell shows none."""
    if not cell.endswith("%"):
        return None
    return 0.0 if cell.startswith("below") else float(cell[:-1])


def _hide_what_subtraction_gives_back(shares: dict[str, list[str]]) -> None:
    """A run's shares add up to 100%, so one hidden cell in a column is 100 minus the rest. When a
    column holds exactly one (a group's, or the creator's in its last form), the smallest other
    group cell is hidden too. The creator's cell is never hidden for this. Three groups other than
    the creator each show a number or "Not shown", so one with a number is always there to hide."""
    for column in range(len(shares[CREATOR_GROUP])):
        hidden = [g for g in GROUP_ORDER if shares[g][column] == NOT_SHOWN_CELL]
        if len(hidden) != 1:
            continue
        others = [
            (_shown_value(shares[g][column]), GROUP_ORDER.index(g), g)
            for g in GROUP_ORDER
            if g != CREATOR_GROUP and _shown_value(shares[g][column]) is not None
        ]
        if others:
            shares[min(others)[2]][column] = NOT_SHOWN_CELL


def _creator_sold(shares: _Shares, precision: Precision) -> str:
    if precision.of_run_totals:
        return (
            f"sold {shares.of_tokens_sold(CREATOR_GROUP)} of all tokens sold in that run and took "
            f"{shares.of_payout(CREATOR_GROUP)} of all NEAR paid out in that run"
        )
    return f"sold {shares.of_supply(CREATOR_GROUP)} of supply"


def chart(limits_off: Run, limits_on: Run) -> str:
    """Price at the end of each minute, both runs on one scale. Words only, no number."""
    width, height, left, top, plot_w, plot_h = 720, 440, 40, 60, 640, 300
    series = [
        ("limits off: no Fair Chance rule", "#2878b5", limits_off.price_by_minute),
        (f"limits on: {LIMITS_ON_LABEL}", "#c8553d", limits_on.price_by_minute),
    ]
    top_price = max((p for _, _, points in series for _, p in points), default=1.0) or 1.0

    def xy(minute: int, price: float) -> str:
        return f"{left + plot_w * minute / 60:.1f},{top + plot_h * (1 - price / top_price):.1f}"

    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}">',
        '<rect width="100%" height="100%" fill="white"/>',
        '<text x="8" y="26" font-size="17" font-weight="bold">'
        "SMDD first hour, replay: price at the end of each minute</text>",
        f'<line x1="{left}" y1="{top + plot_h}" x2="{left + plot_w}" y2="{top + plot_h}" stroke="#444"/>',
        f'<line x1="{left}" y1="{top}" x2="{left}" y2="{top + plot_h}" stroke="#444"/>',
        f'<text x="{left}" y="{top + plot_h + 24}" font-size="12">launch</text>',
        f'<text x="{left + plot_w - 90}" y="{top + plot_h + 24}" font-size="12">end of the hour</text>',
        f'<text x="{left}" y="{top - 8}" font-size="12">price (same scale for both runs)</text>',
    ]
    for index, (label, colour, points) in enumerate(series):
        path = " ".join(xy(minute, price) for minute, price in points)
        parts.append(f'<polyline fill="none" stroke="{colour}" stroke-width="2" points="{path}"/>')
        parts.append(
            f'<text x="{left + 12}" y="{top + plot_h + 48 + 18 * index}" font-size="12" '
            f'fill="{colour}">{label}</text>'
        )
    parts.append("</svg>")
    return "\n".join(parts) + "\n"


def summary(limits_off: Run, limits_on: Run, precision: Precision) -> str:
    off = _Shares(limits_off, precision.decimals)
    creator_on = _creator_on(limits_off, limits_on, precision)
    return (
        f"SMDD first-hour replay. Limits off, {LIMITS_OFF_LABEL}: the creator "
        f"{_creator_sold(off, precision)}. Limits on, {LIMITS_ON_LABEL}, in {LIMITS_ON_RUNNER}: "
        f"{creator_on.phrase}.\n"
    )


def creator_check_line(creator: CreatorCheck) -> str:
    """The creator check as the README gives it. The gap is named by the curve shape only: pump.fun
    charged a 1% fee too, so the fee is not a cause."""
    return (
        f"Real coin: the creator sold {creator.file_percent:.3f}% of supply. Replay with limits off: "
        f"{creator.replay_percent:.2f}%. The gap comes from the curve shape (pump.fun's 1.073 billion "
        "virtual tokens)."
    )


def readme_section(limits_off: Run, limits_on: Run, precision: Precision, creator: CreatorCheck) -> str:
    off = _Shares(limits_off, precision.decimals)
    creator_on = _creator_on(limits_off, limits_on, precision)
    counts = _counts_line(limits_off, limits_on, precision)
    result = (
        f"With the limits off, the creator {_creator_sold(off, precision)} in the first hour. "
        f"With all rules together (early fee, allowance, class and cap), {creator_on.phrase}. "
        "See [the chart and table]"
        "(analysis/smdd-replay/results/)."
    )
    conditions = (
        "SMDD's first-hour trades on Solana, replayed in order on one Fair Chance pool, "
        f"limits on in {LIMITS_ON_RUNNER}. Groups only, minutes from launch. The difference comes "
        "from all the rules together. Sizing in [the replay method](analysis/smdd-replay/METHOD.md)."
    )
    return (
        "README row for the SMDD replay (replaces the marker `<!-- waits on: the SMDD replay "
        "section -->`):\n\n"
        f"| SMDD replay | {result} | {conditions} |\n\n"
        "Under the row:\n\n"
        f"{creator_check_line(creator)}\n\n"
        + (f"{counts}\n\n" if counts else "")
        + f"{DIRECT_FUNDING_LINE}\n"
    )


def render(limits_off: Run, limits_on: Run, precision: Precision, creator: CreatorCheck) -> dict[str, str]:
    return {
        TABLE_FILE: table(limits_off, limits_on, precision),
        CHART_FILE: chart(limits_off, limits_on),
        SUMMARY_FILE: summary(limits_off, limits_on, precision),
        README_FILE: readme_section(limits_off, limits_on, precision, creator),
    }


@dataclass(frozen=True)
class Choice:
    precision: Precision | None
    texts: dict[str, str]
    findings: list[str]  # of the last level tried, when no level passed


def first_passing(
    levels: list[Precision],
    render_at: Callable[[Precision], dict[str, str]],
    findings_for: Callable[[dict[str, str]], list[str]],
) -> Choice:
    """The first precision level whose outputs give no finding, or none and the last findings."""
    findings: list[str] = []
    for level in levels:
        texts = render_at(level)
        findings = findings_for(texts)
        if not findings:
            return Choice(level, texts, [])
    return Choice(None, {}, findings)


def write_texts(texts: dict[str, str], out_dir: Path) -> None:
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    for name, text in texts.items():
        (out_dir / name).write_text(text)
