"""Playing trades through the limits-off pool: trade sizes, block times and the price path.

Trades here are written by hand for each test; none is replay output. The 1 NEAR buy literal
(9_802_950_787_206_654_124_170_709 tokens) is from rule-engine/tests/pool_buy_and_sell.rs:92.
"""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from limits_off_pool import LimitsOffPool  # noqa: E402
from limits_off_replay import blocks_after_launch, play_limits_off  # noqa: E402
from trade_file import Trade  # noqa: E402

ONE_NEAR = 10**24
SOL = 10**9
ONE_NEAR_BUY_TOKENS = 9_802_950_787_206_654_124_170_709
LATE = 60


def trade(account, side, sol_lamports, token_base_units, depth=30 * SOL, seconds=LATE):
    return Trade(account, "other", seconds, side, sol_lamports, token_base_units, depth)


def no_rule_pool():
    return LimitsOffPool.fixed(creator_fee_share_bps=0, early_fee=False)


class LimitsOffReplayTest(unittest.TestCase):
    def test_a_buy_of_0_3_sol_against_30_sol_of_depth_is_1_near_against_100_near(self):
        run = play_limits_off([trade("other-1", "buy", 300_000_000, 1)], no_rule_pool())
        self.assertEqual(run.steps[0].near, ONE_NEAR)
        self.assertEqual(run.steps[0].tokens, ONE_NEAR_BUY_TOKENS)

    def test_the_same_sol_against_twice_the_venue_depth_is_half_the_near(self):
        run = play_limits_off([trade("other-1", "buy", 300_000_000, 1, depth=60 * SOL)], no_rule_pool())
        self.assertEqual(run.steps[0].near, ONE_NEAR // 2)

    def test_a_buy_takes_its_share_of_the_pool_depth_at_that_point(self):
        run = play_limits_off(
            [
                trade("other-1", "buy", 300_000_000, 1),
                trade("other-2", "buy", 300_000_000, 1),
            ],
            no_rule_pool(),
        )
        # After the first buy the pool holds 101 NEAR of depth: 0.3 / 30 of it is 1.01 NEAR.
        self.assertEqual(run.steps[1].near, 1_010_000_000_000_000_000_000_000)

    def test_a_sell_of_half_the_file_balance_sells_half_the_replay_balance(self):
        run = play_limits_off(
            [trade("other-1", "buy", 300_000_000, 1_000), trade("other-1", "sell", 1, 500)],
            no_rule_pool(),
        )
        self.assertEqual(run.steps[1].tokens, ONE_NEAR_BUY_TOKENS // 2)

    def test_a_full_exit_in_the_file_sells_the_whole_replay_balance(self):
        run = play_limits_off(
            [
                trade("other-1", "buy", 300_000_000, 1_000),
                trade("other-1", "sell", 1, 400),
                trade("other-1", "sell", 1, 600),
            ],
            no_rule_pool(),
        )
        self.assertEqual(run.steps[1].tokens + run.steps[2].tokens, ONE_NEAR_BUY_TOKENS)

    def test_with_no_rule_a_buy_in_second_0_pays_only_the_normal_swap_fee(self):
        run = play_limits_off([trade("creator", "buy", 300_000_000, 1, seconds=0)], no_rule_pool())
        self.assertEqual(run.steps[0].fee_near, ONE_NEAR // 100)

    def test_with_the_early_fee_on_a_buy_in_second_0_pays_half(self):
        pool = LimitsOffPool.fixed(creator_fee_share_bps=0, early_fee=True)
        run = play_limits_off([trade("creator", "buy", 300_000_000, 1, seconds=0)], pool)
        self.assertEqual(run.steps[0].fee_near, ONE_NEAR // 2)

    def test_seconds_map_to_blocks_at_0_5789_seconds_per_block(self):
        self.assertEqual(
            [blocks_after_launch(s) for s in (0, 1, 10, 11, 3600)], [0, 1, 17, 19, 6218]
        )

    def test_the_price_path_has_the_last_price_of_every_minute_of_the_hour(self):
        pool = no_rule_pool()
        start = pool.price_near_per_million_tokens()
        run = play_limits_off(
            [
                trade("other-1", "buy", 300_000_000, 1, seconds=61),
                trade("other-2", "buy", 300_000_000, 1, seconds=119),
                trade("other-3", "buy", 300_000_000, 1, seconds=185),
            ],
            pool,
        )
        points = run.price_by_minute
        self.assertEqual([minute for minute, _ in points], list(range(60)))
        # Minute 0 has no trade: the start price. Minute 2 has none: minute 1's last price.
        self.assertEqual(
            [price for _, price in points[:5]],
            [start, run.steps[1].price_after, run.steps[1].price_after, run.steps[2].price_after,
             run.steps[2].price_after],
        )
        self.assertEqual(points[-1][1], run.steps[2].price_after)


if __name__ == "__main__":
    unittest.main()
