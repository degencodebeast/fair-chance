"""Shared helpers for the sniper scene analysis and charts. No chain call, no key.

Integer math in base units: yoctoNEAR (24 decimals) and token units (18 decimals).
"""
from __future__ import annotations

import subprocess
import sys
import tempfile
from fractions import Fraction
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]


class SceneRefused(Exception):
    """The run file cannot back any number: a check failed or the scene did not happen as planned."""


def cite(rel_path: str, pattern: str) -> str:
    """file:line of the first line in the repo file that contains `pattern`. Fails if absent."""
    path = REPO / rel_path
    for number, line in enumerate(path.read_text().splitlines(), start=1):
        if pattern in line:
            return f"{rel_path}:{number}"
    raise SceneRefused(f"citation pattern not found in {rel_path}: {pattern!r} (the source changed)")


def repo_head() -> str:
    try:
        return subprocess.run(
            ["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def drop_percent(near0: int, tok0: int, near1: int, tok1: int) -> Fraction:
    """Exact spot price drop in percent: 100 x (1 - (near1/tok1) / (near0/tok0))."""
    return 100 * (1 - Fraction(near1 * tok0, tok1 * near0))


def fmt_near(yocto: int, places: int = 2) -> str:
    whole, frac = divmod(yocto, 10**24)
    if places == 0:
        return str(whole)
    return f"{whole}.{str(frac).rjust(24, '0')[:places]}"


def short_count(units: int, decimals: int = 18) -> str:
    """A rounded group total in words, e.g. '165.2 million'. Rounds down."""
    tokens = units // 10**decimals
    for size, word in ((10**9, "billion"), (10**6, "million"), (10**3, "thousand")):
        if tokens >= size:
            tenths = tokens * 10 // size
            return f"{tenths // 10}.{tenths % 10} {word}"
    return str(tokens)


# ---- SVG output ---------------------------------------------------------------------------

# Two-series palettes (light on #ffffff, contrast on #000000): each pair keeps 3:1 contrast with
# its background and stays apart for the common colour-vision deficiencies.
THEMES = {
    "light": {"bg": "#ffffff", "fg": "#111111", "muted": "#444444", "grid": "#dddddd",
              "a": "#c2620a", "b": "#1d4ed8"},
    "contrast": {"bg": "#000000", "fg": "#ffffff", "muted": "#e6e6e6", "grid": "#3a3a3a",
                 "a": "#d9732a", "b": "#3d8fe8"},
}
FONT = "Helvetica Neue, Helvetica, Arial, sans-serif"


def esc(text: str) -> str:
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace('"', "&quot;")


def chrome_path() -> str | None:
    """FC_BROWSER_PATH, else a usual Chrome install, else None (then no PNG is made)."""
    import os

    if os.environ.get("FC_BROWSER_PATH"):
        return os.environ["FC_BROWSER_PATH"]
    for candidate in (
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
    ):
        if Path(candidate).exists():
            return candidate
    return None


def svg_to_png(svg_path: Path, png_path: Path, width: int, height: int, chrome: str) -> bool:
    """Renders the SVG to PNG with a local headless Chrome (no network). False on failure."""
    with tempfile.TemporaryDirectory(dir=str(svg_path.parent)) as profile:
        html = Path(profile) / "page.html"
        html.write_text(
            "<!doctype html><html><body style='margin:0;overflow:hidden'>"
            + svg_path.read_text() + "</body></html>"
        )
        result = subprocess.run(
            [chrome, "--headless=new", "--disable-gpu", "--hide-scrollbars",
             f"--user-data-dir={profile}/u", "--no-first-run", "--no-default-browser-check",
             f"--window-size={width},{height}", f"--screenshot={png_path}", html.as_uri()],
            capture_output=True, text=True, timeout=120,
        )
    return result.returncode == 0 and png_path.exists()


def guard_svg(svg_path: Path) -> list[str]:
    """Runs the repo's published-figures guard on the SVG. Returns its findings (locations only)."""
    result = subprocess.run(
        [sys.executable, "-I", str(REPO / "analysis/smdd-replay/published_figures_guard.py"), str(svg_path)],
        capture_output=True, text=True,
    )
    if result.returncode == 0:
        return []
    return (result.stdout + result.stderr).strip().splitlines() or ["guard failed"]
