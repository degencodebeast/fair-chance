#!/usr/bin/env python3
"""Deterministic, synthetic 60-day exercise of Issue #2's stated rules."""
from __future__ import annotations
import argparse, csv, random
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).parent
SUPPLY, VIRTUAL, FEE, ALLOWANCE, CAP, FLOOR = 1_000_000_000., 100., .01, .010153, .05, .001

@dataclass(frozen=True)
class Policy:
    n: int; threshold: float; brake: float | None; penalty: float; window: float = 1.

@dataclass
class Pool:
    token: float = SUPPLY; near: float = VIRTUAL; real: float = 0.
    def quote(self, token: float) -> float: return max(0., self.near - self.token * self.near / (self.token + token))
    def buy(self, near: float, fee: float = FEE) -> float:
        out = self.token - self.token * self.near / (self.near + near * (1 - fee))
        self.token -= out; self.near += near; self.real += near
        return out
    def sell(self, token: float, penalty: float = 0.) -> tuple[float, float]:
        gross = min(self.quote(token), self.real); fee = gross * penalty
        self.token += token; self.near += fee - gross; self.real += fee - gross
        return gross - fee, fee
    def tokens_for(self, near: float) -> float:
        lo, hi = 0., self.token
        for _ in range(64):
            mid = (lo + hi) / 2
            if self.quote(mid) <= near: lo = mid
            else: hi = mid
        return lo

@dataclass
class Wallet:
    name: str; tokens: float = 0.; marked: float = 0.; ever_large: bool = False; spent: float = 0.; received: float = 0.

@dataclass
class Engine:
    policy: Policy; pool: Pool = field(default_factory=Pool); wallets: dict[str, Wallet] = field(default_factory=dict)
    sold: dict[tuple[str, int], float] = field(default_factory=dict); cap: dict[int, float] = field(default_factory=dict)
    marked_sold: dict[int, float] = field(default_factory=dict); prices: dict[int, float] = field(default_factory=dict)
    allowances: dict[tuple[bool, int], float] = field(default_factory=dict)
    brakes: int = 0; penalties: float = 0.; max_drawdown: float = 0.
    def wallet(self, name: str) -> Wallet: return self.wallets.setdefault(name, Wallet(name))
    def bucket(self, hour: int, marked: bool = False) -> int: return int(hour // (24 if marked else self.policy.window))
    def price(self) -> float: return self.pool.near / self.pool.token
    def buy(self, name: str, near: float, hour: int, early: bool = False, block: int = 10_000) -> float:
        fee = .10 - .09 * min(block, self.policy.n) / self.policy.n if early else FEE
        w = self.wallet(name); out = self.pool.buy(near, fee); w.tokens += out; w.marked += out if early else 0.; w.spent += near
        w.ever_large |= w.tokens >= self.policy.threshold * SUPPLY
        return out
    def sell(self, name: str, wanted: float, hour: int) -> tuple[float, float]:
        w = self.wallet(name); marked = w.marked > 0; bucket = self.bucket(hour, marked); allowance = self.allowances.setdefault((marked, bucket), ALLOWANCE * self.pool.token)
        limited = marked or w.ever_large or w.tokens > allowance; prior = self.sold.get((name, bucket), 0.); allowed = min(wanted, w.tokens)
        if marked:
            total_marked = sum(x.marked for x in self.wallets.values())
            share = allowance * w.marked / total_marked if total_marked else 0.
            allowed = min(allowed, max(0., share - prior), max(0., allowance - self.marked_sold.get(bucket, 0.)))
        elif w.ever_large: allowed = min(allowed, max(0., allowance - prior))
        excess = max(0., allowed - max(0., allowance - prior)) if limited and not marked else 0.
        penalty = self.policy.penalty if limited and not marked and not w.ever_large and excess else 0.
        start = self.prices.setdefault(bucket, self.price()); dropped = 1 - self.price() / start; floor = FLOOR * self.pool.token
        cap_bucket = self.bucket(hour)
        if limited: allowed = min(allowed, self.pool.tokens_for(max(self.pool.quote(floor), max(0., CAP * self.pool.real - self.cap.get(cap_bucket, 0.)))))
        if limited and self.policy.brake is not None and dropped >= self.policy.brake: self.brakes += 1; allowed = min(allowed, floor)
        payout, fee = self.pool.sell(allowed, penalty); w.tokens -= allowed; w.marked = max(0., w.marked - allowed); w.received += payout
        self.sold[(name, bucket)] = prior + allowed
        if marked: self.marked_sold[bucket] = self.marked_sold.get(bucket, 0.) + allowed
        if limited: self.cap[cap_bucket] = self.cap.get(cap_bucket, 0.) + payout
        self.penalties += fee
        return allowed, payout
    def split(self, source: str, count: int) -> list[str]:
        w = self.wallet(source)
        if w.ever_large: return []
        names = [f"{source}-{i}" for i in range(count)]; each = w.tokens / count
        for name in names: self.wallet(name).tokens += each
        w.tokens = 0.; return names

def seed(policy: Policy) -> Engine:
    e = Engine(policy); e.buy("creator", 4, 0, True, 0)
    for i in range(5): e.buy(f"early-{i}", 3, 0, i * 10 <= policy.n, i * 10)
    for i in range(3): e.buy(f"large-{i}", 35, 48 + i)
    for i in range(8): e.buy(f"band-{i}", 4, 72 + i)
    return e

def normal(policy: Policy, seed_value: int = 7) -> Engine:
    rng, e, high = random.Random(seed_value), seed(policy), 0.
    for h in range(60 * 24):
        for i in range(12):
            name = f"small-{i}"
            if rng.random() < .58 or e.wallet(name).tokens == 0: e.buy(name, rng.uniform(.03, .35), h)
            else: e.sell(name, e.wallet(name).tokens * rng.uniform(.05, .35), h)
        if h % 6 == 0: e.sell(f"band-{(h // 6) % 8}", e.wallet(f"band-{(h // 6) % 8}").tokens * .15, h)
        if h % 24 == 0: e.sell(f"large-{(h // 24) % 3}", e.wallet(f"large-{(h // 24) % 3}").tokens, h)
        high = max(high, e.price()); e.max_drawdown = max(e.max_drawdown, 1 - e.price() / high)
    return e

def race(policy: Policy) -> dict[str, float]:
    e = seed(policy); a, ao = e.sell("large-0", e.wallet("large-0").tokens, 96); b, bo = e.sell("large-1", e.wallet("large-1").tokens, 96)
    return {"first_tokens": a, "first_near": ao, "second_tokens": b, "second_near": bo, "cap_used": e.cap.get(e.bucket(96), 0.), "brakes": e.brakes}

def split_test(policy: Policy) -> dict[str, float]:
    a, b = seed(policy), seed(policy); a.wallet("band-0").ever_large = False
    direct, direct_near = a.sell("band-0", a.wallet("band-0").tokens, 100); children = b.split("band-0", 5)
    split_near = sum(b.sell(child, b.wallet(child).tokens, 100)[1] for child in children)
    return {"direct_tokens": direct, "direct_near": direct_near, "split_near": split_near, "extra_near": split_near - direct_near, "split_cost_near": len(children) * .007}

def round_trip(policy: Policy) -> dict[str, float]:
    e = seed(policy); before = e.pool.real; bought = e.buy("attacker", 12, 100); _, received = e.sell("attacker", bought, 100)
    return {"near_spent": 12., "near_received": received, "net": received - 12., "cap_after": e.cap.get(e.bucket(100), 0.), "real_before": before, "real_after": e.pool.real}

def creator_exit(policy: Policy) -> dict[str, float]:
    e = seed(policy); start = e.wallet("creator").tokens
    for h in range(60 * 24): e.sell("creator", e.wallet("creator").tokens, h)
    return {"start_tokens": start, "remaining_tokens": e.wallet("creator").tokens, "days_to_exit_or_60": 60 if e.wallet("creator").tokens > 1 else h / 24}

def rows(policy: Policy) -> list[dict[str, object]]:
    base = {"label": "simulation", "n": policy.n, "t": policy.threshold, "brake": "off" if policy.brake is None else policy.brake, "penalty": policy.penalty, "window_hours": policy.window}; e = normal(policy)
    return [dict(base, test="1_normal_brake", brake_trips=e.brakes, max_drawdown=e.max_drawdown), dict(base, test="4_small_holder_drawdown", max_drawdown=e.max_drawdown, final_price=e.price(), penalty_near=e.penalties), dict(base, test="2_first_seller_race", **race(policy)), dict(base, test="3_cap_race", **race(policy)), dict(base, test="5_brake_penalty", brakes=e.brakes, penalty_near=e.penalties), dict(base, test="6_penalty_level", penalty_near=e.penalties), dict(base, test="7_band_split", **split_test(policy)), dict(base, test="8_cap_round_trip", **round_trip(policy)), dict(base, test="9_buy_raise_cap", **round_trip(policy)), dict(base, test="10_creator_exit", **creator_exit(policy))]

def write(name: str, rows: list[dict[str, object]]) -> None:
    path = ROOT / "results" / name; path.parent.mkdir(exist_ok=True); columns = list(dict.fromkeys(k for row in rows for k in row))
    with path.open("w", newline="") as f:
        out = csv.DictWriter(f, fieldnames=columns, extrasaction="ignore", lineterminator="\n"); out.writeheader(); out.writerows(rows)

def main() -> None:
    parser = argparse.ArgumentParser(); parser.add_argument("--quick", action="store_true"); args = parser.parse_args()
    grid = [(n, t, d, p) for n in (5, 17, 50) for t in (.01, .02, .05) for d in (None, .1, .2, .3) for p in (.1, .2, .3)]
    if args.quick: grid = [(17, .02, None, .1)]
    result = [row for n, t, d, p in grid for row in rows(Policy(n, t, d, p))]
    for window in (1 / 6, 24): result.extend(rows(Policy(17, .02, None, .1, window)))
    for test in range(1, 11): write(f"test-{test}.csv", [row for row in result if row["test"].startswith(f"{test}_")])

if __name__ == "__main__": main()
