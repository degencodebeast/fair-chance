"""Plays the trades in order through the limits-off pool, and states how a trade is sized.

Sizing (the same for the limits-off and the limits-on run):
- A buy takes the same share of the Fair Chance pool's NEAR depth (virtual plus real NEAR, just
  before the buy, in that run) as it took of its own venue's SOL depth just before the trade
  (the trade file's `venue_sol_reserve_before`). One function: `near_for_buy`.
- A sell sells the same share of the account's replay balance as the file's sell took of the
  wallet's file balance. A sell of the whole file balance sells the whole replay balance.
- Block: `blocks_after_launch` at SECONDS_PER_BLOCK, an interim value with its source in
  BLOCK_TIME_SOURCE, until a testnet measure replaces it.
"""
from __future__ import annotations

from dataclasses import dataclass

from limits_off_pool import LimitsOffPool
from trade_file import Trade

# NEAR mainnet: 156,266 blocks took 0.5789 s each, from the block heights and timestamps in the
# committed transaction files of analysis/near-replay/fixtures. Interim value.
SECONDS_PER_BLOCK_TEN_THOUSANDTHS = 5789
SECONDS_PER_BLOCK = SECONDS_PER_BLOCK_TEN_THOUSANDTHS / 10_000
BLOCK_TIME_SOURCE = (
    "0.5789 seconds per block: NEAR mainnet, measured from the block "
    "heights and timestamps in the committed transaction files of analysis/near-replay/fixtures "
    "(an interim value until a testnet measure replaces it)"
)
SIZING = (
    "A buy takes the same share of the Fair Chance pool's NEAR depth just before it as it took "
    "of its own venue's SOL depth just before the trade. A sell sells the same share of the "
    "account's replay balance as it sold of its own balance in the trade file. Blocks: "
    + BLOCK_TIME_SOURCE
    + "."
)


def near_for_buy(sol_lamports: int, venue_sol_depth: int, pool_near_depth: int) -> int:
    """The NEAR a buy pays: its share of the venue's SOL depth, of the pool's NEAR depth."""
    return pool_near_depth * sol_lamports // venue_sol_depth


def blocks_after_launch(seconds_from_launch: int) -> int:
    return seconds_from_launch * 10_000 // SECONDS_PER_BLOCK_TEN_THOUSANDTHS


@dataclass(frozen=True)
class Step:
    account: str
    group: str
    minute: int
    side: str
    near: int  # NEAR in for a buy, NEAR out for a sell
    tokens: int
    fee_near: int
    price_after: float
    tokens_asked: int | None = None  # limits on: what the sell asked
    holder_class: str | None = None  # limits on: the rule engine's class (fc_buy, fc_sell)
    fill: str | None = None  # limits on: full, partial or none
    limit_reason: str | None = None  # limits on: why a sell filled in part (fc_sell)
    refusal: str | None = None  # limits on: the refusal code


HOUR_MINUTES = 60


@dataclass(frozen=True)
class Run:
    steps: list[Step]
    total_supply: int
    start_price: float | None = None  # the pool's price before the first trade

    @property
    def price_by_minute(self) -> list[tuple[int, float]]:
        """The price at the end of every minute of the hour (and of any later minute with a trade).
        A minute with no trade keeps the price before it, so no minute is missing."""
        last: dict[int, float] = {}
        for step in self.steps:
            last[step.minute] = step.price_after
        end = max([HOUR_MINUTES - 1, *last])
        price = self.start_price
        if price is None and self.steps:
            price = self.steps[0].price_after
        points = []
        for minute in range(end + 1):
            price = last.get(minute, price)
            points.append((minute, price))
        return points


class SellSizer:
    """Turns a file sell into a replay token amount, from both balances of each account."""

    def __init__(self) -> None:
        self.file_balance: dict[str, int] = {}
        self.replay_balance: dict[str, int] = {}

    def bought(self, trade: Trade, tokens: int) -> None:
        self.file_balance[trade.account] = self.file_balance.get(trade.account, 0) + trade.token_base_units
        self.replay_balance[trade.account] = self.replay_balance.get(trade.account, 0) + tokens

    def tokens_to_sell(self, trade: Trade) -> int:
        file_before = self.file_balance.get(trade.account, 0)
        replay_before = self.replay_balance.get(trade.account, 0)
        if file_before == 0:
            return 0
        if trade.token_base_units >= file_before:
            return replay_before
        return replay_before * trade.token_base_units // file_before

    def sold(self, trade: Trade, tokens: int) -> None:
        file_before = self.file_balance.get(trade.account, 0)
        self.file_balance[trade.account] = max(0, file_before - trade.token_base_units)
        self.replay_balance[trade.account] = self.replay_balance.get(trade.account, 0) - tokens


def play_limits_off(trades: list[Trade], pool: LimitsOffPool) -> Run:
    total_supply = pool.token_reserve
    start_price = pool.price_near_per_million_tokens()
    sizer = SellSizer()
    steps = []
    for trade in trades:
        if trade.side == "buy":
            buy = pool.buy(
                near_for_buy(trade.sol_lamports, trade.venue_sol_reserve_before, pool.near_reserve),
                blocks_after_launch(trade.seconds_from_launch),
            )
            sizer.bought(trade, buy.tokens_out)
            near, tokens, fee_near = buy.near_in, buy.tokens_out, buy.fee_near
        else:
            tokens = sizer.tokens_to_sell(trade)
            sell = pool.sell(tokens)
            sizer.sold(trade, tokens)
            near, fee_near = sell.near_out, sell.fee_near
        steps.append(
            Step(
                account=trade.account,
                group=trade.group,
                minute=trade.seconds_from_launch // 60,
                side=trade.side,
                near=near,
                tokens=tokens,
                fee_near=fee_near,
                price_after=pool.price_near_per_million_tokens(),
            )
        )
    return Run(steps, total_supply, start_price)
