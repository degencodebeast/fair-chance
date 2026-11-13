"""Prints the FAIL and ERROR blocks of a unittest log (scripts/test-trade-replay.sh).

A block of a test that reads the real trade file prints only its first line (the test name) and
the error type: its traceback and message could hold a value, a path or a line of the real data.
"""
import re
import sys
from pathlib import Path

REAL_DATA_TESTS = ("test_repository_holds_no_trade_data",)
SEPARATOR = re.compile(r"^(=+|-+)$")


def blocks(text: str) -> list[list[str]]:
    """Each block from its "FAIL: " or "ERROR: " line up to the next "=" line or the run summary."""
    found, current = [], None
    for line in text.splitlines():
        if line.startswith(("FAIL: ", "ERROR: ")):
            current = [line]
            found.append(current)
        elif current is not None:
            if re.match(r"^=+$", line) or line.startswith("Ran "):
                current = None
            else:
                current.append(line)
    return found


def shown(block: list[str]) -> list[str]:
    name = block[0]
    if not any(test in name for test in REAL_DATA_TESTS):
        return block
    error_type = next(
        (m.group(1) for line in reversed(block) if (m := re.match(r"^(\w+(?:Error|Exception|Exit))\b", line))),
        "an error",
    )
    return [name, f"  {error_type} (details hidden: this test reads the real trade file)"]


def main() -> None:
    text = Path(sys.argv[1]).read_text(errors="replace")
    for block in blocks(text):
        print("\n".join(line for line in shown(block) if not SEPARATOR.match(line)))
        print()


if __name__ == "__main__":
    main()
