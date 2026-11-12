#!/usr/bin/env python3
"""Fetch a fixed Nearly launch sample and render the Issue #1 precheck tables.

This deliberately labels platform-indexed observations as unverified.  It does
not turn them into chain-state facts.  Replace the adapter with the receipt
collector described in DATA_SOURCES.md before declaring a final replay.
"""
from __future__ import annotations

import argparse
import base64
import csv
import hashlib
import json
import time
from collections import defaultdict
from decimal import Decimal
from pathlib import Path
from urllib.error import HTTPError
from urllib.parse import quote
from urllib.request import Request, urlopen

ROOT = Path(__file__).parent
CACHE = ROOT / ".cache"
API = "https://nearly.trade/api"
RPC = "https://archival-rpc.mainnet.fastnear.com"


def get_json(path: str) -> object:
    """Fetch once, retry only 429 with bounded backoff, and cache raw JSON."""
    CACHE.mkdir(exist_ok=True)
    target = f"{API}{path}"
    saved = CACHE / f"{hashlib.sha256(target.encode()).hexdigest()}.json"
    if saved.exists():
        return json.loads(saved.read_text())
    for attempt in range(3):
        try:
            with urlopen(target, timeout=30) as response:
                body = response.read().decode()
            saved.write_text(body)
            return json.loads(body)
        except HTTPError as error:
            if error.code != 429 or attempt == 2:
                raise RuntimeError(f"GET {target}: HTTP {error.code}") from error
            time.sleep(2**attempt)
        except Exception as error:
            raise RuntimeError(f"GET {target}: {error}") from error
    raise AssertionError("unreachable")


def rpc(method: str, params: object) -> object:
    """Read a public archival-RPC result and cache its complete response."""
    return rpc_request({"jsonrpc": "2.0", "id": "fair-chance-replay", "method": method, "params": params})


def rpc_view(account: str, method: str) -> object:
    result = rpc("query", {
        "request_type": "call_function", "account_id": account, "method_name": method,
        "args_base64": "e30=", "finality": "final",
    })
    return json.loads(bytes(result["result"]).decode())


def rpc_request(payload: dict) -> object:
    encoded = json.dumps(payload, separators=(",", ":")).encode()
    CACHE.mkdir(exist_ok=True)
    saved = CACHE / f"rpc-{hashlib.sha256(encoded).hexdigest()}.json"
    if saved.exists():
        response = json.loads(saved.read_text())
    else:
        for attempt in range(5):
            try:
                request = Request(RPC, data=encoded, headers={"content-type": "application/json"})
                with urlopen(request, timeout=30) as handle:
                    raw = handle.read().decode()
                saved.write_text(raw)
                response = json.loads(raw)
                break
            except HTTPError as error:
                if error.code != 429 or attempt == 4:
                    raise RuntimeError(f"RPC {payload['method']}: HTTP {error.code}") from error
                time.sleep(2**attempt)
            except Exception as error:
                raise RuntimeError(f"RPC {payload['method']}: {error}") from error
    if "error" in response:
        raise RuntimeError(f"RPC {payload['method']}: {response['error']}")
    return response["result"]


def principal_from_receipt(trade: dict, token: str) -> tuple[str | None, str]:
    """Use the DEX receipt's declared sender/recipient, never its UI trader field."""
    receipt = rpc("EXPERIMENTAL_receipt", {"receipt_id": trade["receipt"]})
    actions = receipt.get("receipt", {}).get("Action", {}).get("actions", [])
    call = next((x["FunctionCall"] for x in actions if "FunctionCall" in x and x["FunctionCall"]["method_name"] == "ft_on_transfer"), None)
    if not call:
        raise RuntimeError(f"{trade['receipt']}: missing ft_on_transfer action")
    outer = json.loads(base64.b64decode(call["args"]))
    swap = json.loads(outer["msg"]).get("Swap", {})
    sender = outer.get("sender_id")
    recipient = swap.get("swap_out_recipient") or sender
    principal = recipient if trade["side"] == "buy" else sender
    signer = receipt.get("receipt", {}).get("Action", {}).get("signer_id")
    if not sender or not recipient or signer != principal:
        return None, "UNKNOWN_OWNER: receipt signer does not prove swap ownership"
    expected_input = "wrap.near" if trade["side"] == "buy" else token
    if receipt["predecessor_id"] != expected_input:
        raise RuntimeError(f"{trade['receipt']}: input token does not match {trade['side']}")
    return principal, "VERIFIED"


def write_csv(name: str, rows: list[dict[str, object]]) -> None:
    if not rows:
        return
    path = ROOT / "results" / name
    path.parent.mkdir(exist_ok=True)
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]), lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def creator_group(account: str, creator: str) -> bool:
    return account == creator or account.endswith(f".{creator}")


def observed_balance(trades: list[dict], account: str, until_block: int | None = None) -> int:
    balance = high = 0
    for trade in trades:
        if trade["principal"] != account or (until_block is not None and trade["block"] > until_block):
            continue
        delta = int(trade["token_amount"])
        balance += delta if trade["side"] == "buy" else -delta
        high = max(high, balance)
    return high


def groups(trades: list[dict], creator: str, launch_block: int, n: int, threshold_bps: int, supply_raw: int) -> dict[str, str]:
    early = {t["principal"] for t in trades if t["principal"] and t["side"] == "buy" and t["block"] <= launch_block + n}
    accounts = {t["principal"] for t in trades if t["principal"]}
    classified = {}
    for account in accounts:
        if creator_group(account, creator):
            classified[account] = "creator_group"
        elif account in early:
            classified[account] = "other_early_buyers"
        elif observed_balance(trades, account) * 10_000 >= threshold_bps * supply_raw:
            classified[account] = "large_not_early"
        else:
            classified[account] = "everyone_else"
    return classified


def measure_1(coin: dict, trades: list[dict], launch_block: int, scale: int, supply_raw: int) -> list[dict[str, object]]:
    rows = []
    creator = coin["creator"]
    for n in (5, 17, 50):
        early = sum(int(t["token_amount"]) for t in trades if t["side"] == "buy" and t["block"] <= launch_block + n)
        creator_buys = sum(int(t["token_amount"]) for t in trades if t["side"] == "buy" and t["block"] <= launch_block + n and creator_group(t["principal"], creator))
        rows.append({
            "token": coin["token"], "n_blocks": n,
            "launch_block": launch_block, "launch_block_status": "FIRST_VERIFIED_POOL_SWAP_RECEIPT",
            "early_observed_pool_buy_tokens": Decimal(early) / scale,
            "early_observed_pool_buy_share_supply": Decimal(early) / supply_raw,
            "creator_group_observed_pool_buy_tokens": Decimal(creator_buys) / scale,
            "creator_group_observed_pool_buy_share_supply": Decimal(creator_buys) / supply_raw,
            "status": "EARLY_POOL_BUYS_VERIFIED; creator group balance is UNKNOWN without full FT transfer history",
        })
    return rows


def measure_2(coin: dict, trades: list[dict], launch_block: int, scale: int, supply_raw: int) -> list[dict[str, object]]:
    launch_ms, creator = coin["created_at_ms"], coin["creator"]
    rows = []
    for n in (5, 17, 50):
        for threshold_bps in (100, 200, 500):
            classified = groups(trades, creator, launch_block, n, threshold_bps, supply_raw)
            for horizon in ("1h", "24h"):
                cutoff = launch_ms + (3_600_000 if horizon == "1h" else 86_400_000)
                net = defaultdict(int)
                unattributed = 0
                for trade in trades:
                    if trade["ts_ms"] <= cutoff and trade["principal"]:
                        amount = int(trade["token_amount"])
                        net[trade["principal"]] += amount if trade["side"] == "sell" else -amount
                    elif trade["ts_ms"] <= cutoff and trade["side"] == "sell":
                        unattributed += int(trade["token_amount"])
                sold = defaultdict(int)
                for account, amount in net.items():
                    sold[classified[account]] += max(amount, 0)
                total = sum(sold.values())
                rows.append({
                    "token": coin["token"], "n_blocks": n, "t_share_supply": Decimal(threshold_bps) / 10_000,
                    "horizon": horizon, "launch_block": launch_block,
                    "creator_group_net_sold_tokens": Decimal(sold["creator_group"]) / scale,
                    "other_early_buyers_net_sold_tokens": Decimal(sold["other_early_buyers"]) / scale,
                    "large_not_early_net_sold_tokens": Decimal(sold["large_not_early"]) / scale,
                    "everyone_else_net_sold_tokens": Decimal(sold["everyone_else"]) / scale,
                    "unattributed_gross_sold_tokens": Decimal(unattributed) / scale,
                    "all_net_sold_tokens": Decimal(total) / scale,
                    "creator_group_share_all_net_sells": Decimal(sold["creator_group"]) / total if total else 0,
                    "other_early_buyers_share_all_net_sells": Decimal(sold["other_early_buyers"]) / total if total else 0,
                    "large_not_early_share_all_net_sells": Decimal(sold["large_not_early"]) / total if total else 0,
                    "everyone_else_share_all_net_sells": Decimal(sold["everyone_else"]) / total if total else 0,
                    "status": "OWNER_ATTRIBUTION_MIXED; large-holder classification is a lower bound without full FT transfer history",
                })
    return rows


def measure_3(coin: dict, trades: list[dict], launch_block: int, scale: int, supply_raw: int) -> list[dict[str, object]]:
    rows = []
    for n in (5, 17, 50):
        early = {t["principal"] for t in trades if t["principal"] and t["side"] == "buy" and t["block"] <= launch_block + n}
        cutoff = coin["created_at_ms"] + 900_000
        bought = defaultdict(int); sold = defaultdict(int)
        for trade in trades:
            if trade["principal"] in early:
                amount = int(trade["token_amount"])
                if trade["side"] == "buy" and trade["block"] <= launch_block + n: bought[trade["principal"]] += amount
                if trade["side"] == "sell" and trade["ts_ms"] <= cutoff: sold[trade["principal"]] += amount
        fast = [account for account, amount in bought.items() if amount and sold[account] >= .9 * amount]
        fast_buys = sum(bought[account] for account in fast)
        rows.append({"token": coin["token"], "n_blocks": n, "fast_early_seller_count": len(fast),
                     "fast_early_buyer_share_supply": Decimal(fast_buys) / supply_raw,
                     "status": "RECEIPT_OWNER_VERIFIED; direct FT transfers can make this a lower bound"})
    return rows


def measure_4(coin: dict) -> list[dict[str, object]]:
    return [{"token": coin["token"], "n_blocks": n, "t_share_supply": Decimal(t) / 10_000,
             "window": window, "net_or_gross": mode, "ordinary_sellers_above_allowance": "UNKNOWN",
             "share_of_ordinary_sellers_who_carried_tokens": "UNKNOWN",
             "reason": "historical DCL token-reserve snapshots and full FT transfer history are unavailable from the free sources"}
            for n in (5, 17, 50) for t in (100, 200, 500) for window in ("1h", "24h") for mode in ("gross", "net")]


def measure_5(coin: dict) -> dict[str, object]:
    return {"token": coin["token"], "creator_funded_wallet_count": "UNKNOWN",
            "creator_funded_wallet_supply_share": "UNKNOWN", "creator_funded_wallet_sell_share": "UNKNOWN",
            "reason": "first-funder relationships require receipt discovery beyond the public launchpad trade feed"}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--as-of-ms", type=int, required=True, help="Fixed selection timestamp in Unix milliseconds")
    parser.add_argument("--coins", type=int, default=10)
    parser.add_argument("--only", help="Render tables only for this selected token")
    args = parser.parse_args()

    launches = get_json("/launches?sort=new&limit=60&offset=0")
    cutoff = args.as_of_ms - 86_400_000
    selected = [x for x in launches if x["created_at_ms"] <= cutoff][:args.coins]
    if len(selected) != args.coins:
        raise RuntimeError(f"selection produced {len(selected)} coins, expected {args.coins}")

    coin_rows = [{
        "selection_rule": "first 10 Nearly launches in descending created_at_ms at least 24h old; no outcome or volume filter",
        "selection_as_of_ms": args.as_of_ms, "platform": "nearly.trade", "token": x["token"],
        "creator": x["creator"], "launch_time_ms": x["created_at_ms"],
        "pool_id": x["pool_id"], "quote": x["quote"], "pool_type": "DCL pool (platform metadata)",
        "launch_block": "UNKNOWN", "launch_block_reason": "requires receipt-derived creation anchor",
        "reserves": "UNKNOWN", "reserves_reason": "not inferred from UI price/depth",
        "justhoot_status": "UNKNOWN: Issue #1 says link to follow; no source endpoint supplied",
    } for x in selected]
    write_csv("coin-list.csv", coin_rows)

    targets = [x for x in selected if not args.only or x["token"] == args.only]
    if args.only and not targets:
        raise RuntimeError("--only token is outside the fixed selected sample")
    one, two, three, four, five = [], [], [], [], []
    for coin in targets:
        details = get_json(f"/launch/{quote(coin['token'], safe='')}")
        trades = sorted(get_json(f"/trades/{quote(coin['token'], safe='')}?limit=60"), key=lambda x: (x["block"], x["receipt"]))
        if details["trades"] != len(trades):
            raise RuntimeError(f"{coin['token']}: platform returned {len(trades)} of {details['trades']} trades; refusing incomplete precheck")
        for trade in trades:
            trade["principal"], trade["owner_status"] = principal_from_receipt(trade, coin["token"])
        launch_block = trades[0]["block"]
        metadata = rpc_view(coin["token"], "ft_metadata")
        total_supply = int(rpc_view(coin["token"], "ft_total_supply"))
        scale = 10 ** metadata["decimals"]
        one.extend(measure_1(details, trades, launch_block, scale, total_supply))
        two.extend(measure_2(details, trades, launch_block, scale, total_supply))
        three.extend(measure_3(details, trades, launch_block, scale, total_supply))
        four.extend(measure_4(details))
        five.append(measure_5(details))
    write_csv("measure-1-early-supply.csv", one)
    write_csv("measure-2-who-sold.csv", two)
    write_csv("measure-3-fast-early-sellers.csv", three)
    write_csv("measure-4-ordinary-sellers.csv", four)
    write_csv("measure-5-creator-funded-wallets.csv", five)


if __name__ == "__main__":
    main()
