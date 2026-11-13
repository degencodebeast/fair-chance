"""The script's limits-off pool against values the rule engine's own tests assert.

Sources: rule-engine/tests/pool_buy_and_sell.rs and
rule-engine/tests/marked_tokens_and_early_window.rs.
"""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from limits_off_pool import LimitsOffPool  # noqa: E402

ONE_NEAR = 10**24
SUPPLY = 10**27
AFTER_EARLY_WINDOW = 101


class LimitsOffPoolTest(unittest.TestCase):
    def test_a_one_near_buy_after_the_early_window_matches_the_engine(self):
        pool = LimitsOffPool.fixed(creator_fee_share_bps=0, early_fee=True)
        buy = pool.buy(ONE_NEAR, blocks_after_launch=AFTER_EARLY_WINDOW)
        self.assertEqual(buy.fee_near, 10_000_000_000_000_000_000_000)
        self.assertEqual(buy.tokens_out, 9_802_950_787_206_654_124_170_709)
        self.assertEqual(pool.real_near, ONE_NEAR)
        self.assertEqual(pool.token_reserve, SUPPLY - buy.tokens_out)

    def test_selling_the_bought_tokens_back_matches_the_engine(self):
        pool = LimitsOffPool.fixed(creator_fee_share_bps=0, early_fee=True)
        tokens = pool.buy(ONE_NEAR, blocks_after_launch=AFTER_EARLY_WINDOW).tokens_out
        sell = pool.sell(tokens)
        self.assertEqual(sell.fee_near, 9_900_980_295_078_720_665_413)
        self.assertEqual(sell.near_out, 980_197_049_212_793_345_875_828)
        self.assertEqual(pool.token_reserve, SUPPLY)
        self.assertEqual(pool.real_near, 19_802_950_787_206_654_124_172)

    def test_the_early_buy_fee_falls_from_half_at_block_0_to_one_percent_at_block_18(self):
        pool = LimitsOffPool.fixed(creator_fee_share_bps=0, early_fee=True)
        self.assertEqual(pool.buy(ONE_NEAR, blocks_after_launch=0).fee_near, ONE_NEAR // 2)
        self.assertEqual(
            pool.buy(18 * ONE_NEAR, blocks_after_launch=17).fee_near,
            670_000_000_000_000_000_000_000,
        )
        self.assertEqual(pool.buy(ONE_NEAR, blocks_after_launch=18).fee_near, ONE_NEAR // 100)

    def test_the_start_price_is_0_1_near_per_million_tokens(self):
        pool = LimitsOffPool.fixed(creator_fee_share_bps=0, early_fee=True)
        self.assertAlmostEqual(pool.price_near_per_million_tokens(), 0.1)


if __name__ == "__main__":
    unittest.main()
