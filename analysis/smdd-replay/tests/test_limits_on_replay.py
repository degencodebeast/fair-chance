"""The limits-on run (the coin contract in the near-sdk in-process runner) and its equivalence with
the script's limits-off pool.

Trades come from the synthetic fixture; results are fixture results, never replay output.
The runner is a parameter, so the same checks can run against another runner of the coin.
"""
import dataclasses
import os
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from limits_off_pool import LimitsOffPool  # noqa: E402
from limits_off_replay import play_limits_off  # noqa: E402
from limits_on_replay import InProcessCoinRunner, SandboxCoinRunner, plan_for, play_limits_on  # noqa: E402
from trade_file import read_trades  # noqa: E402

FIXTURE = HERE.parent / "fixtures" / "synthetic-trades-fixture.csv"
PAST_THE_EARLY_WINDOW_S = 60


def small_trades_after_the_early_window():
    """The fixture's other-group trades, moved past block N: small buys and sells, no limit."""
    return [
        dataclasses.replace(t, seconds_from_launch=t.seconds_from_launch + PAST_THE_EARLY_WINDOW_S)
        for t in read_trades(FIXTURE)
        if t.group == "other"
    ]


class LimitsOnReplayTest(unittest.TestCase):
    runner = InProcessCoinRunner()

    def test_with_no_limit_triggered_the_script_and_the_coin_give_the_same_price_path(self):
        trades = small_trades_after_the_early_window()
        rules, total_supply = self.runner.rules()
        limits_on = play_limits_on(trades, self.runner)
        self.assertTrue(all(s.refusal is None for s in limits_on.steps))
        self.assertTrue(all(s.fill in (None, "full") for s in limits_on.steps))
        limits_off = play_limits_off(
            trades,
            LimitsOffPool.from_rules(
                rules, total_supply, int(rules["creator_fee_share_bps"]), early_fee=True
            ),
        )
        self.assertEqual(
            [(s.tokens, s.near) for s in limits_off.steps],
            [(s.tokens, s.near) for s in limits_on.steps],
        )
        self.assertEqual(limits_off.price_by_minute, limits_on.price_by_minute)

    def test_the_limits_on_run_takes_every_class_and_refusal_from_the_rule_engine(self):
        limits_on = play_limits_on(read_trades(FIXTURE), self.runner)
        buy, sell = [s for s in limits_on.steps if s.account == "bought-one-percent-1"]
        self.assertEqual(buy.holder_class, "large")
        # Marked tokens cannot be sold in the early window (GLOSSARY.md, "Early window").
        self.assertEqual(sell.refusal, "FC_EARLY_WINDOW")
        self.assertEqual(sell.tokens, 0)



@unittest.skipUnless(
    os.environ.get("FC_REPLAY_ON_SANDBOX") == "1",
    "starts a sandbox: set FC_REPLAY_ON_SANDBOX=1 (verify.sh sandbox mode does)",
)
class SandboxEquivalenceTest(unittest.TestCase):
    """The same trades on a real chain (a fresh sandbox), after block N, where the block does not
    change a fee. The script's pool starts from the sandbox coin's pool before the first trade
    (the launch put the creator's opening buy in it)."""

    def test_after_block_n_the_script_and_the_coin_on_sandbox_give_the_same_tokens_and_near(self):
        trades = small_trades_after_the_early_window()
        answer = SandboxCoinRunner().play_all(plan_for(trades))
        results = answer["results"]
        self.assertTrue(all("refusal" not in r for r in results))
        self.assertTrue(all(r.get("fill") in (None, "full") for r in results))
        rules, start = answer["fc_rules"], answer["start_pool"]
        pool = LimitsOffPool.from_rules(
            rules, int(answer["total_supply"]), int(rules["creator_fee_share_bps"]), early_fee=True
        )
        pool.token_reserve = int(start["token_reserve"])
        pool.real_near = int(start["real_near"])
        self.assertEqual(pool.near_reserve, int(start["near_reserve"]))
        limits_off = play_limits_off(trades, pool)
        self.assertEqual(
            [(s.tokens, s.near) for s in limits_off.steps],
            [(int(r["tokens"]), int(r["near"])) for r in results],
        )
        self.assertEqual(
            [s.price_after for s in limits_off.steps],
            [int(r["near_reserve"]) / int(r["token_reserve"]) for r in results],
        )


if __name__ == "__main__":
    unittest.main()
