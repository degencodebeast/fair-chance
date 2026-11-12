#!/usr/bin/env python3
"""Render a receipt-audited, read-only NEAR launch replay."""
from __future__ import annotations

import argparse
import base64
import csv
import hashlib
import json
import time
from datetime import datetime, timezone
from collections import defaultdict
from decimal import Decimal
from pathlib import Path
from urllib.error import HTTPError
from urllib.parse import quote
from urllib.request import Request, urlopen

ROOT = Path(__file__).parent
CACHE = ROOT / ".cache"
FIXTURES = ROOT / "fixtures"
API = "https://nearly.trade/api"
RPC = "https://archival-rpc.mainnet.fastnear.com"
TX_API = "https://tx.main.fastnear.com/v0"


def get_json(path: str) -> object:
    """Fetch once, retry only 429 with bounded backoff, and cache raw JSON."""
    CACHE.mkdir(exist_ok=True)
    target = f"{API}{path}"
    saved = CACHE / f"{hashlib.sha256(target.encode()).hexdigest()}.json"
    fixture = FIXTURES / saved.name
    if fixture.exists() or saved.exists():
        return json.loads((fixture if fixture.exists() else saved).read_text())
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


def get_url_json(url: str) -> object:
    """Fetch a public JSON URL once and use its exact frozen response thereafter."""
    CACHE.mkdir(exist_ok=True)
    saved = CACHE / f"url-{hashlib.sha256(url.encode()).hexdigest()}.json"
    fixture = FIXTURES / saved.name
    if fixture.exists() or saved.exists():
        return json.loads((fixture if fixture.exists() else saved).read_text())
    try:
        with urlopen(Request(url, headers={"user-agent": "fair-chance-replay/1.0"}), timeout=30) as response:
            raw = response.read().decode()
    except Exception as error:
        raise RuntimeError(f"GET {url}: {error}") from error
    saved.write_text(raw)
    return json.loads(raw)


def rpc(method: str, params: object) -> object:
    """Read a public archival-RPC result and cache its complete response."""
    return rpc_request({"jsonrpc": "2.0", "id": "fair-chance-replay", "method": method, "params": params})


def rpc_view(account: str, method: str) -> object:
    result = rpc("query", {
        "request_type": "call_function", "account_id": account, "method_name": method,
        "args_base64": "e30=", "finality": "final",
    })
    return json.loads(bytes(result["result"]).decode())


def rpc_view_at(account: str, method: str, args: dict, block: int) -> object:
    result = rpc("query", {
        "request_type": "call_function", "account_id": account, "method_name": method,
        "args_base64": base64.b64encode(json.dumps(args, separators=(",", ":")).encode()).decode(), "block_id": block,
    })
    return json.loads(bytes(result["result"]).decode())


def rpc_request(payload: dict) -> object:
    encoded = json.dumps(payload, separators=(",", ":")).encode()
    CACHE.mkdir(exist_ok=True)
    saved = CACHE / f"rpc-{hashlib.sha256(encoded).hexdigest()}.json"
    fixture = FIXTURES / saved.name
    if fixture.exists() or saved.exists():
        response = json.loads((fixture if fixture.exists() else saved).read_text())
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


def tx_api(path: str, payload: dict) -> object:
    """Read FastNEAR's public transaction index and preserve the raw reply."""
    encoded = json.dumps(payload, separators=(",", ":")).encode()
    key = f"tx-{hashlib.sha256((path + encoded.decode()).encode()).hexdigest()}.json"
    CACHE.mkdir(exist_ok=True)
    saved, fixture = CACHE / key, FIXTURES / key
    if fixture.exists() or saved.exists():
        return json.loads((fixture if fixture.exists() else saved).read_text())
    request = Request(f"{TX_API}{path}", data=encoded, headers={"content-type": "application/json"})
    try:
        with urlopen(request, timeout=30) as handle:
            raw = handle.read().decode()
    except Exception as error:
        raise RuntimeError(f"FastNEAR {path}: {error}") from error
    saved.write_text(raw)
    return json.loads(raw)


def launch_creators(tokens: list[str], platform: str) -> dict[str, dict[str, object]]:
    """Return immutable launch anchors and signers from signed launch calls."""
    candidates = {
        token: tx_api("/account", {"account_id": token, "desc": False, "limit": 1})["account_txs"][0]["transaction_hash"]
        for token in tokens
    }
    indexed = tx_api("/transactions", {"tx_hashes": list(candidates.values())})["transactions"]
    transactions = {item["transaction"]["hash"]: item for item in indexed}
    creators = {}
    for token, tx_hash in candidates.items():
        item = transactions.get(tx_hash)
        transaction = item and item["transaction"]
        is_launch = transaction and transaction.get("receiver_id") == platform and any(
            action.get("FunctionCall", {}).get("method_name") == "launch" for action in transaction.get("actions", [])
        )
        if not is_launch:
            raise RuntimeError(f"{token}: earliest indexed transaction is not a signed {platform} launch")
        outcome = item["execution_outcome"]
        creators[token] = {"creator": transaction["signer_id"], "transaction": tx_hash,
                           "block": outcome["block_height"], "time_ms": outcome["block_timestamp"] // 1_000_000}
    return creators


def tx_items(hashes: list[str]) -> dict[str, dict]:
    """Look up at most twenty public transaction records per FastNEAR request."""
    items = {}
    for start in range(0, len(hashes), 20):
        for item in tx_api("/transactions", {"tx_hashes": hashes[start:start + 20]})["transactions"]:
            items[item["transaction"]["hash"]] = item
    return items


def iso_ms(value: str) -> int:
    return int(datetime.fromisoformat(value.replace("Z", "+00:00")).replace(tzinfo=timezone.utc).timestamp() * 1000)


def justhoot_coins(as_of_ms: int, count: int = 5) -> list[dict]:
    rooms = get_url_json("https://api.justhoot.fun/api/rooms?sort=new&limit=20&offset=0&icon_url=true")["rooms"]
    selected = [room for room in rooms if iso_ms(room["created_at"]) <= as_of_ms - 86_400_000][:count]
    if len(selected) != count:
        raise RuntimeError(f"JustHoot selection produced {len(selected)} coins, expected {count}")
    txs = tx_items([room["transaction_hash"] for room in selected])
    coins = []
    for room in selected:
        item = txs.get(room["transaction_hash"])
        if not item:
            raise RuntimeError(f"{room['pool_id']}: launch transaction missing from public index")
        tx, outcome = item["transaction"], item["execution_outcome"]
        if tx["receiver_id"] != "v2-launchpad.justhoot.near":
            raise RuntimeError(f"{room['pool_id']}: transaction does not target JustHoot v2 launchpad")
        trades = get_url_json(f"https://api.justhoot.fun/api/pools/{quote(room['pool_id'], safe='')}/trades?limit=200")
        if trades.get("next_cursor"):
            raise RuntimeError(f"{room['pool_id']}: public trade page is incomplete")
        token = room["token"]
        coins.append({
            "selection_rule": "first 5 JustHoot rooms in descending creation time at least 24h old; no outcome or volume filter",
            "selection_as_of_ms": as_of_ms, "platform": "justhoot.fun", "token": token["id"],
            "creator": tx["signer_id"], "creator_status": "VERIFIED_LAUNCH_TRANSACTION_SIGNER",
            "launch_transaction": tx["hash"], "launch_block": outcome["block_height"],
            "launch_time_ms": outcome["block_timestamp"] // 1_000_000, "launch_anchor": "SIGNED_LAUNCH_TRANSACTION",
            "pool_id": room["pool_id"], "quote": trades["quote"]["id"], "pool_type": "Rhea DCL pool (JustHoot v2 launch receipt)",
            "trades": [{"block": t["block_height"], "receipt": t["transaction_hash"], "side": t["side"],
                        "token_amount": t["token_amount_raw"], "ts_ms": iso_ms(t["timestamp"]),
                        "principal": None, "owner_status": "UNKNOWN_OWNER: JustHoot public feed has no receipt id"}
                       for t in trades["trades"]],
            "first_verified_pool_swap_block": min((t["block_height"] for t in trades["trades"]), default="UNKNOWN"),
            "first_verified_pool_swap_time_ms": min((iso_ms(t["timestamp"]) for t in trades["trades"]), default="UNKNOWN"),
        })
    return coins


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
    return bool(account) and (account == creator or account.endswith(f".{creator}") or creator.endswith(f".{account}"))


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
        unknown_early_buys = sum(int(t["token_amount"]) for t in trades if not t["principal"] and t["side"] == "buy" and t["block"] <= launch_block + n)
        creator_buys = sum(int(t["token_amount"]) for t in trades if t["side"] == "buy" and t["block"] <= launch_block + n and creator_group(t["principal"], creator))
        rows.append({
            "token": coin["token"], "n_blocks": n,
            "launch_block": launch_block, "launch_block_status": "SIGNED_LAUNCH_TRANSACTION",
            "early_observed_pool_buy_tokens": Decimal(early) / scale,
            "early_observed_pool_buy_share_supply": Decimal(early) / supply_raw,
            "creator_group_observed_pool_buy_tokens": Decimal(creator_buys) / scale,
            "creator_group_observed_pool_buy_share_supply": Decimal(creator_buys) / supply_raw,
            "creator_group_received_tokens": "UNKNOWN",
            "creator_group_early_supply_tokens": "UNKNOWN",
            "creator_group_early_supply_share_supply": "UNKNOWN",
            "unattributed_early_buy_tokens": Decimal(unknown_early_buys) / scale,
            "status": "POOL_TRADES_ONLY; creator-group receipts are not indexed, so its early supply is UNKNOWN",
        })
    return rows


def measure_2(coin: dict, trades: list[dict], launch_block: int, scale: int, supply_raw: int) -> list[dict[str, object]]:
    launch_ms, creator = coin["launch_time_ms"], coin["creator"]
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
                known_total = sum(sold.values())
                group_names = ("creator_group", "other_early_buyers", "large_not_early", "everyone_else")
                shares = {group: "UNKNOWN" if unattributed else (Decimal(sold[group]) / known_total if known_total else 0) for group in group_names}
                rows.append({
                    "token": coin["token"], "n_blocks": n, "t_share_supply": Decimal(threshold_bps) / 10_000,
                    "horizon": horizon, "launch_block": launch_block,
                    "creator_group_net_pool_trades_tokens": Decimal(sold["creator_group"]) / scale,
                    "other_early_buyers_net_pool_trades_tokens": Decimal(sold["other_early_buyers"]) / scale,
                    "large_not_early_net_pool_trades_tokens": Decimal(sold["large_not_early"]) / scale,
                    "everyone_else_net_pool_trades_tokens": Decimal(sold["everyone_else"]) / scale,
                    "unattributed_gross_sold_tokens": Decimal(unattributed) / scale,
                    "known_owner_net_pool_trades_tokens": Decimal(known_total) / scale,
                    "creator_group_share": shares["creator_group"],
                    "other_early_buyers_share": shares["other_early_buyers"],
                    "large_not_early_share": shares["large_not_early"],
                    "everyone_else_share": shares["everyone_else"],
                    "status": "POOL_TRADES_ONLY; shares are UNKNOWN when any sell has no proven owner",
                })
    return rows


def measure_3(coin: dict, trades: list[dict], launch_block: int, scale: int, supply_raw: int) -> list[dict[str, object]]:
    rows = []
    for n in (5, 17, 50):
        early = {t["principal"] for t in trades if t["principal"] and t["side"] == "buy" and t["block"] <= launch_block + n}
        cutoff = coin["launch_time_ms"] + 900_000
        bought = defaultdict(int); sold = defaultdict(int)
        for trade in trades:
            if trade["principal"] in early:
                amount = int(trade["token_amount"])
                if trade["side"] == "buy" and trade["block"] <= launch_block + n: bought[trade["principal"]] += amount
                if trade["side"] == "sell" and trade["ts_ms"] <= cutoff: sold[trade["principal"]] += amount
        fast = [account for account, amount in bought.items() if amount and sold[account] >= .9 * amount]
        fast_buys, early_supply = sum(bought[account] for account in fast), sum(bought.values())
        rows.append({"token": coin["token"], "n_blocks": n, "fast_early_seller_count": len(fast),
                     "fast_early_buyer_share_early_supply": Decimal(fast_buys) / early_supply if early_supply else 0,
                     "status": "POOL_TRADES_ONLY; direct FT transfers can make this a lower bound"})
    return rows


def measure_4(coin: dict, launch_block: int, pool_snapshot_block: int) -> list[dict[str, object]]:
    try:
        pool = rpc_view_at("dclv2.ref-labs.near", "get_pool", {"pool_id": coin["pool_id"]}, pool_snapshot_block)
        if not pool:
            raise RuntimeError("pool was not available at the first verified pool-swap block")
        reserve_status, reserve_x, reserve_y = "VERIFIED_GET_POOL", pool.get("total_x"), pool.get("total_y")
    except RuntimeError as error:
        reserve_status, reserve_x, reserve_y = f"ERROR: {error}", "UNKNOWN", "UNKNOWN"
    return [{"token": coin["token"], "n_blocks": n, "t_share_supply": Decimal(t) / 10_000,
             "window": window, "net_or_gross": mode, "ordinary_sellers_above_allowance": "UNKNOWN",
             "share_of_ordinary_sellers_who_carried_tokens": "UNKNOWN",
             "pool_snapshot_block": pool_snapshot_block, "pool_snapshot": reserve_status, "pool_total_x_raw": reserve_x, "pool_total_y_raw": reserve_y,
             "reason": "full per-touch reserve snapshots and FT transfer history remain unavailable"}
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
    nearly = [x for x in launches if x["created_at_ms"] <= cutoff][:args.coins]
    if len(nearly) != args.coins:
        raise RuntimeError(f"Nearly selection produced {len(nearly)} coins, expected {args.coins}")
    selected = nearly + justhoot_coins(args.as_of_ms)

    targets = [x for x in selected if not args.only or x["token"] == args.only]
    if args.only and not targets:
        raise RuntimeError("--only token is outside the fixed selected sample")
    nearly_targets = [coin for coin in targets if coin["token"] in {x["token"] for x in nearly}]
    creators = launch_creators([coin["token"] for coin in nearly_targets], "nearlytrade.near")
    one, two, three, four, five, coin_rows = [], [], [], [], [], []
    for coin in targets:
        if coin.get("platform") == "justhoot.fun":
            trades, creator, launch_tx, launch_block = coin["trades"], coin["creator"], coin["launch_transaction"], coin["launch_block"]
        else:
            details = get_json(f"/launch/{quote(coin['token'], safe='')}")
            trades = sorted(get_json(f"/trades/{quote(coin['token'], safe='')}?limit=60"), key=lambda x: (x["block"], x["receipt"]))
            if details["trades"] != len(trades):
                raise RuntimeError(f"{coin['token']}: platform returned {len(trades)} of {details['trades']} trades; refusing incomplete precheck")
            for trade in trades:
                trade["principal"], trade["owner_status"] = principal_from_receipt(trade, coin["token"])
            launch = creators[coin["token"]]
            creator, launch_tx, launch_block = launch["creator"], launch["transaction"], launch["block"]
            coin["creator"], coin["launch_time_ms"] = creator, launch["time_ms"]
        coin_rows.append({
            "selection_rule": coin.get("selection_rule", "first 10 Nearly launches in descending created_at_ms at least 24h old; no outcome or volume filter"),
            "selection_as_of_ms": args.as_of_ms, "platform": coin.get("platform", "nearly.trade"), "token": coin["token"],
            "creator": creator, "creator_status": coin.get("creator_status", "VERIFIED_LAUNCH_TRANSACTION_SIGNER"), "launch_transaction": launch_tx,
            "launch_block": launch_block, "launch_time_ms": coin["launch_time_ms"], "launch_anchor": "SIGNED_LAUNCH_TRANSACTION",
            "first_verified_pool_swap_block": coin.get("first_verified_pool_swap_block", trades[0]["block"]),
            "first_verified_pool_swap_time_ms": coin.get("first_verified_pool_swap_time_ms", trades[0]["ts_ms"]),
            "pool_id": coin["pool_id"], "quote": coin["quote"], "pool_type": coin.get("pool_type", "Rhea DCL pool (verified get_pool)"),
        })
        metadata = rpc_view(coin["token"], "ft_metadata")
        total_supply = int(rpc_view(coin["token"], "ft_total_supply"))
        scale = 10 ** metadata["decimals"]
        one.extend(measure_1(coin, trades, launch_block, scale, total_supply))
        two.extend(measure_2(coin, trades, launch_block, scale, total_supply))
        three.extend(measure_3(coin, trades, launch_block, scale, total_supply))
        four.extend(measure_4(coin, launch_block, trades[0]["block"]))
        five.append(measure_5(coin))
    write_csv("coin-list.csv", coin_rows)
    write_csv("measure-1-early-supply.csv", one)
    write_csv("measure-2-who-sold.csv", two)
    write_csv("measure-3-fast-early-sellers.csv", three)
    write_csv("measure-4-ordinary-sellers.csv", four)
    write_csv("measure-5-creator-funded-wallets.csv", five)


if __name__ == "__main__":
    main()
