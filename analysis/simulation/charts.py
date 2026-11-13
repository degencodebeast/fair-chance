"""Render compact SVG summaries for simulation tests 1, 4, and 10."""
import csv
from pathlib import Path

ROOT = Path(__file__).parent

def chart(test, metrics, title):
    rows = list(csv.DictReader((ROOT / "results" / f"test-{test}.csv").open()))
    rows = [r for r in rows if r.get(metrics[0]) not in (None, "")]
    values = [(f"s={float(r['creator_fee_share']):.0%}", float(r[metrics[0]])) for r in rows]
    if metrics[1]:
        values += [(f"sniper s={float(r['creator_fee_share']):.0%}", float(r[metrics[1]])) for r in rows]
    if not values: return
    width, height, base = 700, 90 + 30 * len(values), 55
    maximum = max(v for _, v in values) or 1
    bars = [f'<text x="8" y="{base+i*30}" font-size="13">{label}</text><rect x="160" y="{base-15+i*30}" width="{480*v/maximum:.1f}" height="18" fill="#2878b5"/><text x="{166+480*v/maximum:.1f}" y="{base+i*30}" font-size="12">{v:.4g}</text>' for i,(label,v) in enumerate(values)]
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}"><rect width="100%" height="100%" fill="white"/><text x="8" y="28" font-size="18" font-weight="bold">{title}</text>{"".join(bars)}</svg>\n'
    (ROOT / "results" / f"test-{test}.svg").write_text(svg)

chart(1, ("max_drawdown", None), "Test 1: 60-day max drawdown")
chart(4, ("max_drawdown", None), "Test 4: small-holder drawdown")
chart(10, ("days", "sniper_days"), "Test 10: creator exit days, with and without snipers")
