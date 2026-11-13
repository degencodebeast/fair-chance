"""The coin's pool with every sell limit off: the same integer math as the rule engine.

Mirrors rule-engine/src/pool_math.rs and rule-engine/src/early_buy_fee.rs. Every rounding goes in
the pool's favour: tokens and NEAR out round down, fees round up, the creator share rounds down.
The creator share s is a parameter. The falling early buy fee can be turned off: the replay's
limits-off run has no Fair Chance rule at all, while the equivalence test keeps the fee on.
"""
from __future__ import annotations

from dataclasses import dataclass

BPS = 10_000
ONE_NEAR = 10**24


def _mul_div(a: int, b: int, c: int) -> int:
    return a * b // c


def _mul_div_up(a: int, b: int, c: int) -> int:
    return -(-a * b // c)


@dataclass(frozen=True)
class Buy:
    near_in: int
    fee_near: int
    creator_fee_near: int
    tokens_out: int


@dataclass(frozen=True)
class Sell:
    tokens_in: int
    near_out: int
    fee_near: int
    creator_fee_near: int


@dataclass
class LimitsOffPool:
    """One coin's pool. Values are the `fc_rules` names; amounts are yoctoNEAR and token units."""

    virtual_near: int
    swap_fee_bps: int
    early_fee_start_bps: int
    early_blocks: int
    creator_fee_share_bps: int
    early_fee: bool
    token_reserve: int
    real_near: int = 0

    @classmethod
    def from_rules(
        cls, rules: dict, total_supply: int, creator_fee_share_bps: int, early_fee: bool
    ) -> "LimitsOffPool":
        """A fresh pool from a coin's `fc_rules` answer and its `ft_total_supply`."""
        return cls(
            virtual_near=int(rules["virtual_near"]),
            swap_fee_bps=int(rules["swap_fee_bps"]),
            early_fee_start_bps=int(rules["early_fee_start_bps"]),
            early_blocks=int(rules["early_blocks"]),
            creator_fee_share_bps=creator_fee_share_bps,
            early_fee=early_fee,
            token_reserve=total_supply,
        )

    @classmethod
    def fixed(cls, creator_fee_share_bps: int, early_fee: bool) -> "LimitsOffPool":
        """A fresh pool with the values of `FixedValues::fixed()`."""
        return cls(
            virtual_near=100 * ONE_NEAR,
            swap_fee_bps=100,
            early_fee_start_bps=5000,
            early_blocks=17,
            creator_fee_share_bps=creator_fee_share_bps,
            early_fee=early_fee,
            token_reserve=10**27,
        )

    @property
    def near_reserve(self) -> int:
        return self.virtual_near + self.real_near

    def price_near_per_million_tokens(self) -> float:
        """yoctoNEAR (24 decimals) per token unit (18 decimals) is NEAR per million tokens."""
        return self.near_reserve / self.token_reserve

    def buy_fee(self, near_in: int, blocks_after_launch: int) -> int:
        """fee(b) = f + (start - f) x (N + 1 - b) / (N + 1) for b = 0 to N, else f. Rounded up."""
        steps = self.early_blocks + 1
        normal = self.swap_fee_bps
        if not self.early_fee or blocks_after_launch >= steps:
            return _mul_div_up(near_in, normal, BPS)
        start = max(self.early_fee_start_bps, normal)
        numerator = normal * steps + (start - normal) * (steps - blocks_after_launch)
        return _mul_div_up(near_in, numerator, BPS * steps)

    def _creator_fee(self, normal_fee_near: int) -> int:
        return _mul_div(normal_fee_near, self.creator_fee_share_bps, BPS)

    def buy(self, near_in: int, blocks_after_launch: int) -> Buy:
        fee_near = self.buy_fee(near_in, blocks_after_launch)
        # The creator share is of the normal swap fee only, never of the early-fee extra.
        creator_fee_near = self._creator_fee(_mul_div_up(near_in, self.swap_fee_bps, BPS))
        near_after_fee = near_in - fee_near
        tokens_out = _mul_div(self.token_reserve, near_after_fee, self.near_reserve + near_after_fee)
        self.token_reserve -= tokens_out
        self.real_near += near_in - creator_fee_near
        return Buy(near_in, fee_near, creator_fee_near, tokens_out)

    def sell(self, tokens_in: int) -> Sell:
        # NEAR reserve minimum: the payout never takes the NEAR reserve below virtual P.
        near_before_fee = min(
            _mul_div(self.near_reserve, tokens_in, self.token_reserve + tokens_in),
            self.real_near,
        )
        fee_near = _mul_div_up(near_before_fee, self.swap_fee_bps, BPS)
        near_out = near_before_fee - fee_near
        creator_fee_near = self._creator_fee(fee_near)
        self.token_reserve += tokens_in
        self.real_near -= near_out + creator_fee_near
        return Sell(tokens_in, near_out, fee_near, creator_fee_near)
