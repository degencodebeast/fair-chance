#!/usr/bin/env python3
"""The guard over every published output of the replay, every caption and the video script.

Before the rules run, a text is normalized: NFKC (fullwidth forms, circled letters), every
unicode digit as an ASCII digit, unicode dashes as "-", no zero-width character, lower case.

A text fails when it shows any of these:
- an absolute clock time: 14:05, 14 : 05, 14.05.32, 14h05, 21h, 1405Z, 9pm, 9 p.m., an ISO date
  with a time, or a unix time. Minutes or blocks after launch are relative and pass;
- a long code (an address, a signature or a hash): one word of 32 characters or more, or
  letter-and-digit pieces that add up to 32 or more across spaces, "-" or "_";
- a venue word in any spelling (pump.fun, pump fun, Raydium, curve, bonding, migration,
  graduation, Jupiter, Meteora, Orca). Only the builder's creator check line may name the curve;
- a raw count: a whole number of five or more digits, with or without separators. A year, a small
  block or minute number and a decimal part pass;
- the size of a trade: an amount with a unit or a currency sign (SOL, wSOL, NEAR, lamports,
  tokens, coins, USDC, SMDD, $), also in number words or with spaced digits;
- prose about one trader with a time point (a second, a minute, a block) or a share: "a buyer",
  "the first buyer", "one wallet", "one sold". The creator is a role, not a trader here;
- anything per wallet: a table row that is not a group, a role account name in any spelling, a
  numbered wallet ("wallet 7", "buyer #7"), a named account (alice.near), a ranking (top ten, the
  largest buyer, the biggest buy) or a single-trade example.
Only group totals and the chart are published; the groups are below.

METHOD.md describes the method, so it may name the method's venues; every other rule holds there.

In an SVG chart every visible or accessible text counts: element text (tspan, textPath, desc,
title, metadata), comments, CDATA and attribute values, with entities decoded. Only the drawing
attributes (coordinates, sizes, colours) do not count.
A finding names the file, the line and, in a table, the column number. It never shows the text.

One exception, by exact value. If `evidence/testnet-run.json` exists and says "network":
"testnet", the guard lets through its coin code hash, its factory and coin accounts and each
transaction hash in it, and no other field. A value counts only
when the whole word matches; a changed character, a longer word or a mainnet account fails. The
guard reads that file and nothing else. It makes no network call. Without the file, nothing is
let through.

Command (a video script or a caption file goes through it too):
  python3 -I analysis/smdd-replay/published_figures_guard.py <file>...
It exits 1 on any finding.
"""
from __future__ import annotations

import html
import json
import re
import sys
import unicodedata
from pathlib import Path

GROUPS = (
    "creator",
    "funded directly by the same wallet before launch",
    "bought 1% of supply or more in the first hour",
    "other",
)
# The README results row, the one table row that is not a group.
README_ROW = "SMDD replay"
ROW_LABELS = GROUPS + (README_ROW,)
# The builder's creator check line names the launch curve as the cause of the gap.
CREATOR_CHECK_LINE = re.compile(
    r"Real coin: the creator sold \d+(?:\.\d+)?% of supply\. Replay with limits off: "
    r"\d+(?:\.\d+)?%\. The gap comes from the curve shape \(pump\.fun's 1\.073 billion virtual "
    r"tokens\)\."
)
# A group phrase that names one wallet as the funder, not as a trader.
FUNDER_PHRASE = re.compile(r"funded directly by (?:one|the same) wallet")

_NUMBER_WORDS = (
    "zero|one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|twenty|thirty|forty|"
    "fifty|hundred|thousand|million|billion|point|half"
)
_NUMBER = rf"(?:\d[\d,\s]*(?:\.\d+)?|(?:{_NUMBER_WORDS})(?:[\s-]+(?:{_NUMBER_WORDS}))*)"
_TRADER = (
    r"(?:buyer|seller|trader|wallet|holder|account|address|person|sniper|whale|purchaser|"
    r"participant|investor|user|bot)"
)

CLOCK_TIME = re.compile(
    r"(?<![\d.])\d{1,2}\s*:\s*\d{2}(?!\d)"
    r"|(?<![\d.])\d{1,2}\.\d{2}\.\d{2}(?![\d.])"
    r"|(?<![\w.])\d{1,2}h(?:\d{2})?\b"
    r"|(?<![\w.])\d{4}\s?z\b"
    r"|(?<![\w.])\d{1,2}\s*[ap]\.?\s?m\b"
    r"|\d{4}-\d{2}-\d{2}[t\s]\d"
    r"|(?<!\d)1\d{9}(?:\d{3})?(?!\d)"
)
# A whole number of five or more digits, bare or with thousands separators ("," "_" "'" space, or
# "." twice). A decimal part does not count, so 0.5789 passes; a year has four digits.
RAW_NUMBER = re.compile(r"(?<![\d.,])(?:\d{5,}|\d{1,3}(?:[,_' ]\d{3})+|\d{1,3}(?:\.\d{3}){2,})(?!\d)")
LONG_WORD = re.compile(r"\w{32,}")
_CODE_PIECE = r"(?=[a-z]*\d)(?=\d*[a-z])[a-z0-9]{6,}"
SPLIT_CODE = re.compile(rf"(?<![a-z0-9]){_CODE_PIECE}(?:[\s_-]+{_CODE_PIECE})+(?![a-z0-9])")
_SEP = r"[\s._-]*"
VENUE = re.compile(
    rf"\bpump{_SEP}(?:fun|swap)\b|\bray{_SEP}dium\b|\bcurv\w*|\bbond(?:ing|ed)\b|\bmigrat\w*"
    r"|\bgraduat\w*|\bjupiter\b|\bmeteora\b|\borca\b|\bmoonshot\b"
)
TRADE_SIZE = re.compile(
    rf"(?<![\w.]){_NUMBER}\s*(?:[kmb]|mm|bn|thousand|million|billion)?\s*"
    r"(?:w?sol|solana|w?near|lamports?|yocto\w*|tokens?|coins?|usd[ct]?|dollars?|smdd|n\b|◎)(?!\w)"
    r"|[$◎€£]\s*\d"
)
ONE_TRADER = re.compile(
    rf"\b(?:a|an|one|the|this|that|single|some|any)\s+(?!creator)(?:[a-z-]+\s+)?{_TRADER}\b"
    r"|\bone\s+(?:bought|sold|paid|took|held|sells|buys|dumped|exited)\b"
)
TIME_POINT = re.compile(
    r"\bseconds?\b|\bsecs?\b|(?<![\w.])\d+(?:\.\d+)?\s?s\b|\bminutes?\s*#?\d"
    r"|\d+(?:\.\d+)?\s*(?:minutes?|mins?)\b|\bmin\s*\d|\bblocks?\s*#?\d|\d+\s*blocks?\b|\bslot\s*\d"
)
SHARE = re.compile(r"\d\s*%|\bpercent\b|\beverything\b|\bit all\b")
PER_WALLET = re.compile(
    r"\b(?:other|same[-_]?funder|bought[-_]?one[-_]?percent|o|sf)[-_]?\d+\b"
    rf"|\b{_TRADER}\s*(?:#|no\.?\s*|number\s+)?\d+\b"
    r"|\b[a-z0-9][\w-]*\.(?:near|testnet|sol|eth)\b"
    r"|\btop[\s_-]*(?:\d+|one|two|three|four|five|six|seven|eight|nine|ten|twenty|fifty|hundred)\b"
    r"|\b(?:largest|biggest|smallest|highest|lowest|top|heaviest|richest|leading)\b"
    r"(?:[\s-]+[a-z]+){0,2}?[\s-]+(?:buy|purchase|sell|sale|seller|buyer|wallet|holder|trader|"
    r"account|trade|order|position|whale)s?\b"
)
EXAMPLE = re.compile(r"\b(?:for example|for instance|e\.g\.|suppose|imagine|say,?\s+(?:a|an|one|the))\b")
TRADE_WORD = re.compile(r"\b(?:buy|bought|buys|sell|sold|sells|trade|trades|paid|purchase)\b")
SENTENCE_END = re.compile(r"(?<=[.!?])\s+")
METHOD_FILE = "METHOD.md"
_DASHES = dict.fromkeys(map(ord, "‐‑‒–—―−﹣"), "-")
_INVISIBLE = dict.fromkeys(map(ord, "­​‌‍⁠﻿"), None)


EVIDENCE_FILE = Path(__file__).resolve().parents[2] / "evidence" / "testnet-run.json"
_HASH_SHAPE = re.compile(r"[1-9A-HJ-NP-Za-km-z]{43,44}|[0-9a-fA-F]{64}")
_ACCOUNT_SHAPE = re.compile(r"[a-z0-9][a-z0-9_-]*(?:\.[a-z0-9_-]+)*\.testnet")


def load_allowlist(path: Path = EVIDENCE_FILE) -> frozenset:
    """The exact values the guard lets through, from the testnet run record: the coin code hash
    ("code_hash"), the factory and coin accounts ("factory", "coin") and each "tx_hash" in
    "transactions". Only when the file says "network": "testnet", and only these fields, each
    with the shape of its kind. A missing, unreadable or other file gives an empty set, which is
    the guard without an exception."""
    try:
        data = json.loads(Path(path).read_text())
    except (OSError, ValueError):
        return frozenset()
    if not isinstance(data, dict) or data.get("network") != "testnet":
        return frozenset()
    found = []
    if _is(data.get("code_hash"), _HASH_SHAPE):
        found.append(data["code_hash"])
    for field in ("factory", "coin"):
        if _is(data.get(field), _ACCOUNT_SHAPE):
            found.append(data[field])
    transactions = data.get("transactions")
    for tx in transactions if isinstance(transactions, list) else []:
        if isinstance(tx, dict) and _is(tx.get("tx_hash"), _HASH_SHAPE):
            found.append(tx["tx_hash"])
    return frozenset(found)


def _is(value, shape) -> bool:
    return isinstance(value, str) and shape.fullmatch(value) is not None


def _without_allowed(text: str, allowed: frozenset) -> str:
    for value in allowed:
        edge = r"(?<![A-Za-z0-9_.-])" + re.escape(value) + r"(?![A-Za-z0-9_-]|\.[A-Za-z0-9])"
        text = re.sub(edge, " ", text)
    return text


def normalized(text: str) -> str:
    """NFKC, ASCII digits, plain dashes, no zero-width character, lower case."""
    text = unicodedata.normalize("NFKC", text).translate(_DASHES).translate(_INVISIBLE)
    text = "".join(str(unicodedata.decimal(c)) if c.isdecimal() and not c.isascii() else c for c in text)
    return text.lower()


def _raw_count(text: str) -> bool:
    return any(sum(c.isdigit() for c in m.group()) >= 5 for m in RAW_NUMBER.finditer(text))


def _long_code(text: str) -> bool:
    if LONG_WORD.search(text):
        return True
    return any(len(re.sub(r"[\s_-]", "", m.group())) >= 32 for m in SPLIT_CODE.finditer(text))


def _sentence_fails(sentence: str) -> bool:
    if ONE_TRADER.search(sentence) and (TIME_POINT.search(sentence) or SHARE.search(sentence)):
        return True
    return bool(EXAMPLE.search(sentence) and TRADE_WORD.search(sentence))


def text_fails(text: str, venues_allowed: bool = False, allowed: frozenset = frozenset()) -> bool:
    """True when this piece of text (a line or a table cell) breaks a rule."""
    text = normalized(_without_allowed(CREATOR_CHECK_LINE.sub(" ", text), allowed))
    text = FUNDER_PHRASE.sub(" ", text)
    rules = (CLOCK_TIME, TRADE_SIZE, PER_WALLET) + (() if venues_allowed else (VENUE,))
    if any(rule.search(text) for rule in rules) or _long_code(text) or _raw_count(text):
        return True
    return any(_sentence_fails(s) for s in SENTENCE_END.split(text))


def _is_table_row(line: str) -> bool:
    return line.lstrip().startswith("|")


def _is_separator(row: str) -> bool:
    return _is_table_row(row) and set(row.replace("|", "").strip()) <= set("-: ")


def findings_in_text(
    name: str, text: str, venues_allowed: bool = False, allowed: frozenset = frozenset()
) -> list[str]:
    """One finding per failing line, as "<name>: line N", or per failing table cell, as
    "<name>: line N, column C". A table's header is its first row when a separator follows it;
    every other row must be a group (or the README row)."""
    findings = []
    lines = text.splitlines()
    for number, line in enumerate(lines, 1):
        if not _is_table_row(line):
            if text_fails(line, venues_allowed, allowed):
                findings.append(f"{name}: line {number}")
            continue
        if _is_separator(line):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        starts_table = number == 1 or not _is_table_row(lines[number - 2])
        is_header = starts_table and number < len(lines) and _is_separator(lines[number])
        for index, cell in enumerate(cells):
            not_a_group = index == 0 and not is_header and cell not in ROW_LABELS
            if not_a_group or text_fails(cell, venues_allowed, allowed):
                findings.append(f"{name}: line {number}, column {index + 1}")
    return findings


# Drawing attributes: coordinates, sizes and colours. Every other attribute value counts as text.
_DRAWING_ATTRIBUTES = {
    "x", "y", "x1", "y1", "x2", "y2", "cx", "cy", "r", "rx", "ry", "dx", "dy", "width", "height",
    "points", "d", "transform", "viewbox", "fill", "stroke", "stroke-width", "font-size",
    "font-weight", "font-family", "text-anchor", "dominant-baseline", "opacity", "xmlns", "version",
}
_SVG_PART = re.compile(r"<!--(.*?)-->|<!\[CDATA\[(.*?)\]\]>|<([^>]*)>|([^<]+)", re.S)
_ATTRIBUTE = re.compile(r"([\w:.-]+)\s*=\s*(?:\"([^\"]*)\"|'([^']*)')")


def visible_text(name: str, text: str) -> str:
    """What a reader or a screen reader gets. For an SVG chart: every element text, comment, CDATA
    and attribute value except the drawing attributes, entities decoded, one per line with its
    spaces joined. Other files: as they are."""
    if not name.endswith(".svg"):
        return text
    shown = []
    for comment, cdata, tag, words in _SVG_PART.findall(text):
        if tag:
            if tag.startswith(("?", "!")):
                continue
            for attribute, double, single in _ATTRIBUTE.findall(tag):
                if attribute.lower() not in _DRAWING_ATTRIBUTES:
                    shown.append(double or single)
        else:
            shown.append(comment or cdata or words)
    joined = (" ".join(html.unescape(part).split()) for part in shown)
    return "\n".join(part for part in joined if part) + "\n"


def findings_in_files(paths: list[Path], allowed: frozenset = frozenset()) -> list[str]:
    findings = []
    for path in paths:
        name = Path(path).name
        text = visible_text(name, Path(path).read_text())
        findings += findings_in_text(name, text, venues_allowed=name == METHOD_FILE, allowed=allowed)
    return findings


def main() -> None:
    if len(sys.argv) < 2:
        print("published figures guard: name the files", file=sys.stderr)
        sys.exit(2)
    findings = findings_in_files([Path(p) for p in sys.argv[1:]], load_allowlist())
    for finding in findings:
        print(f"published figures guard: failed at {finding}")
    if findings:
        sys.exit(1)
    print(f"published figures guard: passed ({len(sys.argv) - 1} file(s))")


if __name__ == "__main__":
    main()
