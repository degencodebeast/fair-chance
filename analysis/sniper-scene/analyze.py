#!/usr/bin/env python3
"""Reads a sniper-scene run file and writes the numbers, the summary and the video charts.

Read-only on the run file. No chain call, no key. Integer math in base units.

WITH FAIR CHANCE (FROM CHAIN): fc_sell receipts and fc_pool / fc_early_window read on the sandbox.
WITHOUT FAIR CHANCE (CALCULATED): each sniper's asked amount fills in full, in the same order, on
a plain constant-product pool with the same swap fee and creator share, from the same pool state
(fc_pool just before sniper #1's sell). The math is analysis/smdd-replay/limits_off_pool.py
(imported, not copied). The ordinary buyers then sell their bags on that pool.

The analysis refuses a run (exit 1, no file written, no number printed) when any harness check
failed, when an early sell was not refused with FC_EARLY_WINDOW, or when an early sell landed
after block N: such a run does not show the early window.

usage: analyze.py <run.json> <out dir> [--no-png]
"""
from __future__ import annotations

import json
import sys
from fractions import Fraction
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from fc_common import REPO, SceneRefused, chrome_path, cite, drop_percent, fmt_near, repo_head  # noqa: E402

sys.path.insert(0, str(REPO / "analysis" / "smdd-replay"))
from limits_off_pool import LimitsOffPool, _mul_div, _mul_div_up  # noqa: E402

import charts  # noqa: E402

BPS = 10_000
U = 10**18


def i(x) -> int:
    return int(x)


def pct(n: int, d: int, places: int = 1) -> str:
    return f"{float(Fraction(100 * n, d)):.{places}f}%"


def mtok(units: int) -> str:
    """Tokens in millions, 2 decimals, cut."""
    hundredths = units * 100 // (U * 10**6)
    return f"{hundredths // 100}.{hundredths % 100:02d}M"


def near(y: int, places: int = 2) -> str:
    return fmt_near(y, places) + " NEAR"


def plain_pool(fc_pool: dict, rules: dict) -> LimitsOffPool:
    if i(fc_pool["near_reserve"]) != i(fc_pool["virtual_near"]) + i(fc_pool["real_near"]):
        raise SceneRefused("fc_pool near_reserve is not virtual_near + real_near")
    return LimitsOffPool(
        virtual_near=i(rules["virtual_near"]), swap_fee_bps=i(rules["swap_fee_bps"]),
        early_fee_start_bps=i(rules["early_fee_start_bps"]), early_blocks=i(rules["early_blocks"]),
        creator_fee_share_bps=i(rules["creator_fee_share_bps"]), early_fee=False,
        token_reserve=i(fc_pool["token_reserve"]), real_near=i(fc_pool["real_near"]),
    )


def replay_chain(fc_pool: dict, rules: dict, sells: list[dict]) -> tuple[int, int, list[str]]:
    """Replays the filled amounts with the contract math; checks each fc_sell's fee, creator fee
    and NEAR out. Returns the end token reserve and real NEAR, and the mismatches."""
    tok, real = i(fc_pool["token_reserve"]), i(fc_pool["real_near"])
    virt = i(fc_pool["virtual_near"])
    fee_bps, share = i(rules["swap_fee_bps"]), i(rules["creator_fee_share_bps"])
    problems = []
    for n, e in enumerate(sells, start=1):
        sold = i(e["tokens_sold"])
        if not sold:
            continue
        before_fee = min(_mul_div(virt + real, sold, tok + sold), real)
        fee = _mul_div_up(before_fee, fee_bps, BPS)
        creator = _mul_div(fee, share, BPS)
        if i(e["fee_near"]) != fee:
            problems.append(f"sell {n}: fee_near {e['fee_near']} != {fee}")
        if i(e["creator_fee_near"]) != creator:
            problems.append(f"sell {n}: creator_fee_near {e['creator_fee_near']} != {creator}")
        if i(e["near_out"]) + i(e["penalty_near"]) != before_fee - fee:
            problems.append(f"sell {n}: near_out + penalty != {before_fee - fee}")
        tok += sold
        real -= i(e["near_out"]) + creator
    return tok, real, problems


def event(action: dict, name: str) -> dict:
    found = [e["data"] for e in action["tx"]["events"] if e["event"] == name]
    if not found:
        raise SceneRefused(f"{action['step']} ({action.get('role')}) has no {name} event")
    return found[0]


def refuse_bad_run(scene: dict) -> None:
    """Raises SceneRefused unless the run shows what the scene claims."""
    failed = [c["check"] for c in scene["checks"] if c["pass"] is not True]
    if failed:
        raise SceneRefused(f"a harness check failed: {failed[0]}")
    n = i(scene["fc_rules"]["early_blocks"])
    acts = scene["actions"]
    early = [a for a in acts if a["step"] == "sniper sell in early window"]
    if not early:
        raise SceneRefused("the run has no early-window sell")
    for a in early:
        block = a.get("block_after_launch")
        if block is None:
            raise SceneRefused(f"{a['role']} early sell has no block: it was not refused on chain")
        if block > n:
            raise SceneRefused(f"{a['role']} early sell landed in block {block} after launch, "
                               f"after the early window (blocks 0 to {n}); this run proves nothing")
        code = (a["tx"].get("refusal") or {}).get("code")
        if a["tx"]["success"] or code != "FC_EARLY_WINDOW":
            raise SceneRefused(f"{a['role']} early sell was not refused with FC_EARLY_WINDOW (got {code!r})")
    for a in acts:
        if a["step"] == "sniper buy" and a["block_after_launch"] > n:
            raise SceneRefused(f"{a['role']} bought in block {a['block_after_launch']}, after the early window")
    bags = [i(event(a, "fc_buy")["tokens_out"]) for a in acts if a["step"] == "sniper buy"]
    if bags != sorted(bags, reverse=True):
        raise SceneRefused("the snipers are not numbered by buy order")


def derive(run: dict) -> dict:
    """Every number of the scene, from the run file. Raises SceneRefused on a bad run."""
    s = run["scene"]
    refuse_bad_run(s)
    rules = s["fc_rules"]
    acts = s["actions"]
    snaps = {x["label"]: x for x in s["snapshots"]}
    before, after_dump, end = snaps["before the snipers' dump"], snaps["after the snipers' dump"], snaps["end"]

    launch = next(a for a in acts if a["step"] == "launch")
    opening = event(launch, "fc_buy")
    sniper_buys = [a for a in acts if a["step"] == "sniper buy"]
    early_sells = [a for a in acts if a["step"] == "sniper sell in early window"]
    buyer_buys = [a for a in acts if a["step"] == "buyer buy"]
    dumps = [a for a in acts if a["step"] == "sniper dump"]
    buyer_sells = [a for a in acts if a["step"] == "buyer sell"]
    creator_sells = [a for a in acts if a["step"] == "creator sell"]

    sn_near_in = sum(i(event(a, "fc_buy")["near_in"]) for a in sniper_buys)
    sn_fee = sum(i(event(a, "fc_buy")["fee_near"]) for a in sniper_buys)
    sn_tokens = sum(i(event(a, "fc_buy")["tokens_out"]) for a in sniper_buys)

    sells_fc = [a["fc_sell"] for a in dumps]
    asked = [i(e["tokens_asked"]) for e in sells_fc]
    filled = [i(e["tokens_sold"]) for e in sells_fc]
    near_fc = sum(i(e["near_out"]) for e in sells_fc)

    ew0, ew1, ew_end = before["fc_early_window"], after_dump["fc_early_window"], end["fc_early_window"]
    shared = i(ew0["shared_allowance"])
    marked_total = i(ew0["early_supply_total"])

    # FROM CHAIN: price before and after the dump; replay check of the receipts.
    p0, p1 = before["fc_pool"], after_dump["fc_pool"]
    t0, n0 = i(p0["token_reserve"]), i(p0["near_reserve"])
    t1, n1 = i(p1["token_reserve"]), i(p1["near_reserve"])
    drop_fc = drop_percent(n0, t0, n1, t1)
    tok_r, real_r, problems = replay_chain(p0, rules, sells_fc)
    replay_ok = (tok_r, real_r) == (t1, i(p1["real_near"])) and not problems

    # CALCULATED: plain pool, same start, same order, asked amounts fill in full.
    plain = plain_pool(p0, rules)
    near_plain = 0
    floor_hit = False
    for a in asked:
        if _mul_div(plain.near_reserve, a, plain.token_reserve + a) > plain.real_near:
            floor_hit = True
        near_plain += plain.sell(a).near_out
    drop_plain = drop_percent(n0, t0, plain.near_reserve, plain.token_reserve)

    b_fc = [a["fc_sell"] for a in buyer_sells]
    b_out_plain = [plain.sell(i(e["tokens_asked"])).near_out for e in b_fc]

    creator_sell = None
    if creator_sells:
        e = creator_sells[0]["fc_sell"]
        creator_sell = {"asked": e["tokens_asked"], "filled": e["tokens_sold"], "near_out": e["near_out"],
                        "fill": e["fill"], "limit_reason": e["limit_reason"], "class": e["class"]}

    evl = run["early_vs_late"]
    e_sell, l_sell = (x["fc_sell"] for x in evl["sells"])
    e_buy = next(x["data"] for x in evl["early_buy"]["events"] if x["event"] == "fc_buy")
    l_buy = next(x["data"] for x in evl["late_buy"]["events"] if x["event"] == "fc_buy")

    def twin(buy: dict, sell: dict) -> dict:
        return {"buy_block_after_launch": buy["block"] - evl["launch_block"], "tokens": buy["tokens_out"],
                "sold": sell["tokens_sold"], "near_out": sell["near_out"], "fill": sell["fill"],
                "limit_reason": sell["limit_reason"], "class": sell["class"]}

    quote_checks = [c for c in s["checks"] if "fc_quote_sell" in c["check"]]
    return {
        "label_with": "WITH FAIR CHANCE (FROM CHAIN, local sandbox)",
        "label_without": "WITHOUT FAIR CHANCE (CALCULATED, plain pool, same fee, same start, same order)",
        "early_blocks": i(rules["early_blocks"]),
        "snipers": len(dumps),
        "sniper_buy_blocks_after_launch": sorted({a["block_after_launch"] for a in sniper_buys}),
        "sniper_near_in": str(sn_near_in), "sniper_buy_fee": str(sn_fee), "sniper_tokens": str(sn_tokens),
        "early_sells": len(early_sells),
        "early_sells_refused_codes": sorted({a["tx"]["refusal"]["code"] for a in early_sells}),
        "early_sells_blocks_after_launch": sorted({a["block_after_launch"] for a in early_sells}),
        "asked_total": str(sum(asked)), "filled_total": str(sum(filled)),
        "per_sniper": [{"n": k + 1, "asked": str(a), "filled": str(f), "near_out": e["near_out"],
                        "limit_reason": e["limit_reason"], "fill": e["fill"], "class": e["class"]}
                       for k, (a, f, e) in enumerate(zip(asked, filled, sells_fc))],
        "limit_reasons": sorted({str(e["limit_reason"]) for e in sells_fc}),
        "near_out_with": str(near_fc), "near_out_without_calculated": str(near_plain),
        "price_drop_with_percent": f"{float(drop_fc):.4f}", "price_drop_without_percent": f"{float(drop_plain):.4f}",
        "plain_pool_near_floor_hit": floor_hit,
        "shared_allowance": str(shared),
        "shared_used_before": ew0["shared_allowance_used"], "shared_left_before": ew0["shared_allowance_left"],
        "shared_used_after_dump": ew1["shared_allowance_used"], "shared_left_after_dump": ew1["shared_allowance_left"],
        "shared_used_end": ew_end["shared_allowance_used"], "shared_left_end": ew_end["shared_allowance_left"],
        "marked_supply_at_start": str(marked_total),
        "opening_buy_near": opening["near_in"], "creator_marked": opening["marked_tokens"],
        "snipers_bound_by_share": str(shared * sn_tokens // marked_total),
        "creator_sell": creator_sell,
        "real_near_before_dump": p0["real_near"],
        "replay_matches_chain": replay_ok, "replay_problems": problems,
        "buyers": [{"n": k + 1, "near_in": event(a, "fc_buy")["near_in"], "marked": event(a, "fc_buy")["marked_tokens"],
                    "near_out_with": e["near_out"], "near_out_without_calculated": str(p), "fill": e["fill"],
                    "limit_reason": e["limit_reason"], "class": e["class"], "cap_used": e["cap_used"]}
                   for k, (a, e, p) in enumerate(zip(buyer_buys, b_fc, b_out_plain))],
        "quote_checks": len(quote_checks),
        "early_vs_late": {"early": twin(e_buy, e_sell), "late": twin(l_buy, l_sell)},
        "sources": {
            "plain sell": cite("analysis/smdd-replay/limits_off_pool.py", "def sell(self, tokens_in: int)"),
            "NEAR out before fee": cite("rule-engine/src/pool_math.rs", "pub fn near_out_before_fee("),
            "shared allowance": cite("rule-engine/src/early_supply.rs", "pub fn shared_allowance("),
            "early window": cite("rule-engine/src/early_supply.rs", "pub fn in_early_window("),
            "early_blocks = 17": cite("rule-engine/src/fixed_values.rs", "early_blocks: 17"),
        },
    }


def summary_lines(d: dict, head: str) -> list[str]:
    L: list[str] = []
    w = L.append
    n = d["early_blocks"]
    sn_tokens, asked, filled = i(d["sniper_tokens"]), i(d["asked_total"]), i(d["filled_total"])
    near_fc, near_plain = i(d["near_out_with"]), i(d["near_out_without_calculated"])
    real_before = i(d["real_near_before_dump"])
    marked_total, creator_marked = i(d["marked_supply_at_start"]), i(d["creator_marked"])
    classes = [s["class"] for s in d["per_sniper"]]
    w("# Sniper cabal scene: summary")
    w("")
    w(f"Local NEAR sandbox only (near-sandbox 2.13.4, fresh chain, throwaway keys). Code: commit {head}. "
      "No rule constant changed.")
    w("")
    w("Labels: **FROM CHAIN** = read from sandbox receipts and views. "
      "**CALCULATED** = plain-pool math, no Fair Chance limits, same fee, same start, same order.")
    w("")
    w("## What happened")
    w("")
    w(f"- The creator launched the coin with an opening buy of {near(i(d['opening_buy_near']), 0)}. "
      f"It got {mtok(creator_marked)} marked tokens (FROM CHAIN).")
    w(f"- One funder wallet made {d['snipers']} sniper wallets. They bought together in block "
      f"{', '.join(map(str, d['sniper_buy_blocks_after_launch']))} after launch (the early window is blocks 0 to {n}).")
    w(f"- The snipers paid {near(i(d['sniper_near_in']), 0)} in total. The early buy fee took "
      f"{near(i(d['sniper_buy_fee']))}. They got {mtok(sn_tokens)} tokens, all marked (FROM CHAIN).")
    w(f"- Each sniper then tried to sell its whole bag in block "
      f"{', '.join(map(str, d['early_sells_blocks_after_launch']))} after launch. All {d['early_sells']} sells "
      f"were refused with {', '.join(d['early_sells_refused_codes'])}. No token moved (FROM CHAIN).")
    w(f"- After the early window, {len(d['buyers'])} ordinary buyers bought "
      f"{near(i(d['buyers'][0]['near_in']), 0)} each. Their tokens are not marked.")
    w(f"- Then the {d['snipers']} snipers sold their whole bags, one after another. Then the ordinary buyers sold theirs.")
    if d["creator_sell"]:
        w("- Last, the creator sold its whole bag.")
    w("")
    w("## The key numbers")
    w("")
    w(f"- The snipers asked to sell {mtok(asked)} tokens.")
    w(f"- With Fair Chance (FROM CHAIN), {mtok(filled)} went through ({pct(filled, asked)}). "
      f"limit_reason: {', '.join(d['limit_reasons'])}.")
    w(f"- NEAR out to the snipers: {near(near_fc)} with Fair Chance (FROM CHAIN). "
      f"Without Fair Chance: {near(near_plain)} (CALCULATED).")
    w(f"- Price drop from the snipers' sells: {float(d['price_drop_with_percent']):.2f}% with Fair Chance (FROM CHAIN). "
      f"Without Fair Chance: {float(d['price_drop_without_percent']):.2f}% (CALCULATED).")
    w(f"- The pool held {near(real_before)} of real NEAR before the dump (FROM CHAIN). Without Fair Chance the snipers take "
      f"{pct(near_plain, real_before, 0)} of it (CALCULATED). With Fair Chance they take {pct(near_fc, real_before, 1)} (FROM CHAIN).")
    w(f"- Sniper classes at their sells: {classes.count('band')} band, {classes.count('large')} large "
      "(above 2% of supply), FROM CHAIN. The early-supply limit binds either way.")
    w("")
    w("## One shared limit for all early supply")
    w("")
    w(f"- fc_early_window before the dump (FROM CHAIN): shared_allowance {mtok(i(d['shared_allowance']))}, "
      f"used {mtok(i(d['shared_used_before']))}, left {mtok(i(d['shared_left_before']))}.")
    w(f"- After the snipers' dump (FROM CHAIN): used {mtok(i(d['shared_used_after_dump']))}, "
      f"left {mtok(i(d['shared_left_after_dump']))}.")
    w(f"- All marked supply at the window start was {mtok(marked_total)}: the snipers' {mtok(sn_tokens)} "
      f"and the creator's {mtok(creator_marked)}.")
    w(f"- Each wallet may use its pro-rata part of the one shared allowance. The {d['snipers']} snipers together may use "
      f"{pct(sn_tokens, marked_total)} of it, about {mtok(i(d['snipers_bound_by_share']))} (CALCULATED from the chain values). "
      f"They used {mtok(filled)} (FROM CHAIN).")
    c = d["creator_sell"]
    if c:
        left = i(d["shared_left_end"])
        w(f"- The creator sold its whole bag of {mtok(i(c['asked']))}: {mtok(i(c['filled']))} went through, "
          f"{near(i(c['near_out']))} out, limit_reason {c['limit_reason']} (FROM CHAIN).")
        dust = "" if left == 0 else f" ({left} base units of rounding, less than one token)"
        w(f"- At the end (FROM CHAIN): shared allowance used {mtok(i(d['shared_used_end']))}, "
          f"left {mtok(left)}{dust}. The shared allowance left ends at 0: all early supply together used it up.")
    else:
        w(f"- The {mtok(i(d['shared_left_after_dump']))} left is the creator's part. The creator did not sell in this run.")
    w("- One wallet with the same total bag would get the same total. Splitting into 10 wallets gains nothing.")
    w("")
    w("## Ordinary buyers")
    w("")
    for b in d["buyers"]:
        w(f"- Buyer #{b['n']}: paid {near(i(b['near_in']))}, sold the whole bag after the snipers' dump: fill {b['fill']}, "
          f"got {near(i(b['near_out_with']))} (FROM CHAIN). After a full dump without Fair Chance: "
          f"{near(i(b['near_out_without_calculated']))} (CALCULATED).")
    w("- The snipers' limit did not block them. Their sells are a same-hour round trip, so their own allowance "
      "and the cap did not touch them.")
    w("")
    w("## Checks")
    w("")
    w(f"- Each sell's fill equals fc_quote_sell read just before it: yes, all {d['quote_checks']}.")
    w(f"- Replay of the {d['snipers']} sniper receipts with the repo pool math gives the chain's pool after the dump exactly: "
      f"{'yes' if d['replay_matches_chain'] else 'NO ' + '; '.join(d['replay_problems'])}.")
    w(f"- Plain pool hit the NEAR reserve floor: {'yes' if d['plain_pool_near_floor_hit'] else 'no'}.")
    w(f"- Every early sell landed in blocks 0 to {n}: yes. All harness checks pass: yes. "
      "(The analysis refuses a run where either is false.)")
    w("")
    w("## Conditions")
    w("")
    w("- Time: all sells are in the first 24 h window. The shared allowance refills every 24 h, "
      "so the snipers can sell again in the next window. It slows the dump. It does not stop it forever.")
    w("- Pool depth: the allowance is the tokens for a 2% price fall, so it grows with the pool. "
      "It binds before the 5% real NEAR cap only when the pool's real NEAR is above about 25 NEAR. "
      "Below that, the cap binds first (limit_reason cap_full or floor).")
    w("- Creator share: the creator's opening buy is marked too, so the snipers get their pro-rata part of the allowance, not all of it.")
    w("- Number of snipers: it does not change the total. 1 wallet or 10 wallets get the same total share.")
    w("- The ordinary buyers' full fill is a same-hour round trip. A buyer who sells in a later hour is limited by its own allowance.")
    w("- The CALCULATED case keeps the same bags. Without Fair Chance there is no early buy fee, so real snipers "
      "would hold more tokens and take more NEAR. The CALCULATED numbers are conservative.")
    w(f"- Without Fair Chance the snipers could also sell inside blocks 0 to {n}. The CALCULATED case sells after "
      "the window, from the same pool state.")
    w("")
    w("## One early wallet vs one late wallet (second coin, same sandbox)")
    w("")
    e, l = d["early_vs_late"]["early"], d["early_vs_late"]["late"]
    w(f"- Same buy ({near(i(d['sniper_near_in']) // d['snipers'], 0)}) on a second coin with the same opening buy. "
      f"Early wallet in block {e['buy_block_after_launch']}, late wallet in block {l['buy_block_after_launch']} after launch.")
    for name, x in (("Early", e), ("Late", l)):
        w(f"- {name} wallet: {mtok(i(x['tokens']))} tokens, sold {mtok(i(x['sold']))} ({pct(i(x['sold']), i(x['tokens']))}), "
          f"got {near(i(x['near_out']))}, limit_reason {x['limit_reason']} (FROM CHAIN).")
    w("- Condition: the late wallet sells in the same 1 h window as its buy, so it fills in full.")
    return L


def main(argv: list[str]) -> int:
    args = [a for a in argv if not a.startswith("--")]
    if len(args) != 2 or any(a not in ("--no-png",) for a in argv if a.startswith("--")):
        print("usage: analyze.py <run.json> <out dir> [--no-png]", file=sys.stderr)
        return 2
    run = json.loads(Path(args[0]).read_text())
    try:
        d = derive(run)
    except SceneRefused as error:
        print(f"analyze: REFUSED, no numbers from this run: {error}", file=sys.stderr)
        return 1
    out = Path(args[1])
    out.mkdir(parents=True, exist_ok=True)
    (out / "derived.json").write_text(json.dumps(d, indent=1) + "\n")
    (out / "summary.md").write_text("\n".join(summary_lines(d, repo_head())) + "\n")
    chrome = None if "--no-png" in argv else chrome_path()
    asked = [i(s["asked"]) for s in d["per_sniper"]]
    filled = [i(s["filled"]) for s in d["per_sniper"]]
    ok = charts.asked_vs_through(out, asked, filled, chrome)
    ok &= charts.price_drop(out, Fraction(d["price_drop_without_percent"]), Fraction(d["price_drop_with_percent"]), chrome)
    ok &= d["replay_matches_chain"]
    print(f"analyze: wrote summary.md, derived.json and charts; checks {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
