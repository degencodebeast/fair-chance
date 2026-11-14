"""The two video charts for the sniper scene, 1920x1080, light and high-contrast, SVG + PNG.

(a) asked vs went through, per sniper wallet #1..#10. Default: no axis numbers, totals in the
    caption. A *_numbers copy with tick values is for checking only.
(b) price drop from the snipers' sells, two bars: without Fair Chance (CALCULATED) and with
    Fair Chance (FROM CHAIN).
Every SVG goes through the repo's published-figures guard.
"""
from __future__ import annotations

from fractions import Fraction
from pathlib import Path

from fc_common import FONT, THEMES, esc, guard_svg, short_count, svg_to_png

W, H = 1920, 1080
U = 10**18


def nice_step(top: int) -> int:
    raw = max(top // 5, 1)
    power = 10 ** (len(str(raw)) - 1)
    for m in (1, 2, 5, 10):
        if m * power >= raw:
            return m * power
    return 10 * power


def tick_label(tokens: int) -> str:
    for size, suffix in ((10**9, "B"), (10**6, "M"), (10**3, "K")):
        if tokens >= size:
            v = tokens / size
            return f"{v:.0f}{suffix}" if v == int(v) else f"{v:.1f}{suffix}"
    return str(tokens)


def bar_path(x: float, w: float, top_y: float, base: float, color: str) -> str:
    h = base - top_y
    if h <= 0:
        return ""
    r = min(4, h)
    return (f'<path d="M{x:.1f},{base} V{top_y + r:.1f} Q{x:.1f},{top_y:.1f} {x + r:.1f},{top_y:.1f} '
            f'H{x + w - r:.1f} Q{x + w:.1f},{top_y:.1f} {x + w:.1f},{top_y + r:.1f} V{base} Z" fill="{color}"/>')


def header(t: dict, title: str, subtitle: str) -> list[str]:
    return [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
            f'<rect width="{W}" height="{H}" fill="{t["bg"]}"/>',
            f'<g font-family="{FONT}" fill="{t["fg"]}">',
            f'<text x="190" y="100" font-size="54" font-weight="700">{esc(title)}</text>',
            f'<text x="190" y="160" font-size="34" fill="{t["muted"]}">{esc(subtitle)}</text>',
            f'<text x="{W - 70}" y="100" font-size="34" font-weight="700" text-anchor="end">local sandbox</text>']


def write(out: Path, stem: str, svg: str, chrome: str | None) -> bool:
    """Writes the SVG (and the PNG when `chrome` is set). True when the guard passes."""
    path = out / f"{stem}.svg"
    path.write_text(svg)
    made = chrome is not None and svg_to_png(path, out / f"{stem}.png", W, H, chrome)
    findings = guard_svg(path)
    png = " + png" if made else (" (PNG failed)" if chrome else " (no PNG)")
    print(f"  {stem}: svg{png}; published-figures guard {'PASS' if not findings else 'FAIL'}")
    for f in findings:
        print(f"    {f}")
    return not findings and (chrome is None or made)


def asked_vs_through(out: Path, asked: list[int], filled: list[int], chrome: str | None) -> bool:
    ok = True
    for theme in ("light", "contrast"):
        for numbers in (False, True):
            ok &= write(out, f"a_asked_vs_through_{theme}" + ("_numbers" if numbers else ""),
                        render_a(THEMES[theme], asked, filled, numbers), chrome)
    return ok


def render_a(t: dict, asked: list[int], filled: list[int], show_numbers: bool) -> str:
    n = len(asked)
    whole = [a // U for a in asked]
    step = nice_step(max(whole))
    top = -(-max(whole) // step) * step
    left, right, plot_top, plot_bottom = 190, 70, 260, 860
    plot_w, plot_h = W - left - right, plot_bottom - plot_top
    group_w = plot_w / n
    bar_w = min(group_w * 0.36, 60)
    gap = 2

    def y(units: int) -> float:
        return plot_bottom - plot_h * (units / U) / top

    o = header(t, "Asked vs went through, per wallet",
               "Sniper wallets funded by one wallet dump their whole bags after the early window")
    lx = 190
    for k, (key, label) in enumerate((("a", "asked"), ("b", "went through"))):
        x = lx + k * 220
        o.append(f'<rect x="{x}" y="196" width="30" height="30" rx="4" fill="{t[key]}"/>')
        o.append(f'<text x="{x + 42}" y="221" font-size="30">{label}</text>')
    for k in range(0, top + 1, step):
        yy = plot_bottom - plot_h * k / top
        o.append(f'<line x1="{left}" x2="{W - right}" y1="{yy:.1f}" y2="{yy:.1f}" stroke="{t["grid"]}" stroke-width="2"/>')
        if show_numbers:
            o.append(f'<text x="{left - 18}" y="{yy + 11:.1f}" font-size="30" text-anchor="end" fill="{t["muted"]}">{tick_label(k)}</text>')
    o.append(f'<text transform="translate(70 {(plot_top + plot_bottom) / 2}) rotate(-90)" font-size="36" '
             f'text-anchor="middle" fill="{t["muted"]}">tokens</text>')
    for k in range(n):
        cx = left + group_w * (k + 0.5)
        o.append(bar_path(cx - bar_w - gap / 2, bar_w, y(asked[k]), plot_bottom, t["a"]))
        o.append(bar_path(cx + gap / 2, bar_w, y(filled[k]), plot_bottom, t["b"]))
        # A sliver still shows: at least a 4 px mark in the went-through colour.
        if plot_bottom - y(filled[k]) < 4:
            o.append(f'<rect x="{cx + gap / 2:.1f}" y="{plot_bottom - 4}" width="{bar_w:.1f}" height="4" fill="{t["b"]}"/>')
        o.append(f'<text x="{cx:.1f}" y="{plot_bottom + 46}" font-size="30" text-anchor="middle" fill="{t["muted"]}">#{k + 1}</text>')
    o.append(f'<line x1="{left}" x2="{W - right}" y1="{plot_bottom}" y2="{plot_bottom}" stroke="{t["fg"]}" stroke-width="2"/>')
    o.append(f'<text x="{left + plot_w / 2}" y="{plot_bottom + 92}" font-size="30" text-anchor="middle" fill="{t["muted"]}">'
             "sniper wallets, in sell order</text>")
    a_tot, f_tot = sum(asked), sum(filled)
    share = f_tot * 1000 // a_tot
    caption = (f"Tokens in total: asked {short_count(a_tot)}, went through {short_count(f_tot)} "
               f"({share // 10}.{share % 10}%), from chain")
    o.append(f'<text x="{left}" y="{H - 50}" font-size="40" font-weight="700">{esc(caption)}</text>')
    o.append("</g></svg>")
    return "\n".join(x for x in o if x)


def price_drop(out: Path, without: Fraction, with_fc: Fraction, chrome: str | None) -> bool:
    ok = True
    for theme in ("light", "contrast"):
        ok &= write(out, f"b_price_drop_{theme}", render_b(THEMES[theme], without, with_fc), chrome)
    return ok


def render_b(t: dict, without: Fraction, with_fc: Fraction) -> str:
    left, right, plot_top, plot_bottom = 190, 70, 260, 820
    plot_h = plot_bottom - plot_top
    top = 10 * (int(max(without, with_fc)) // 10 + 1)
    o = header(t, "Price drop when the snipers dump",
               "Same sells, same starting pool, same order, same swap fee")
    for k in range(0, top + 1, 10):
        yy = plot_bottom - plot_h * k / top
        o.append(f'<line x1="{left}" x2="{W - right}" y1="{yy:.1f}" y2="{yy:.1f}" stroke="{t["grid"]}" stroke-width="2"/>')
        o.append(f'<text x="{left - 18}" y="{yy + 11:.1f}" font-size="30" text-anchor="end" fill="{t["muted"]}">{k}%</text>')
    bar_w = 360
    slots = [(W * 0.36, without, t["a"], "Without Fair Chance", "CALCULATED: every asked token fills"),
             (W * 0.68, with_fc, t["b"], "With Fair Chance", "FROM CHAIN: the real sandbox fills")]
    for cx, value, color, label, sub in slots:
        top_y = plot_bottom - plot_h * float(value) / top
        o.append(bar_path(cx - bar_w / 2, bar_w, top_y, plot_bottom, color))
        o.append(f'<text x="{cx}" y="{top_y - 22:.1f}" font-size="64" font-weight="700" text-anchor="middle">{float(value):.1f}%</text>')
        o.append(f'<text x="{cx}" y="{plot_bottom + 60}" font-size="40" font-weight="700" text-anchor="middle">{esc(label)}</text>')
        o.append(f'<text x="{cx}" y="{plot_bottom + 108}" font-size="30" text-anchor="middle" fill="{t["muted"]}">{esc(sub)}</text>')
    o.append(f'<line x1="{left}" x2="{W - right}" y1="{plot_bottom}" y2="{plot_bottom}" stroke="{t["fg"]}" stroke-width="2"/>')
    o.append(f'<text x="{left}" y="{H - 50}" font-size="36" font-weight="700">'
             "Spot price of the pool, before the first sniper sell and after the last.</text>")
    o.append("</g></svg>")
    return "\n".join(x for x in o if x)
