import base64
import importlib.util
import json
import sys
from decimal import Decimal
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[1]

def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module

replay = load("replay", "analysis/near-replay/replay.py")
sim = load("simulate", "analysis/simulation/simulate.py")


class ReplayTests(unittest.TestCase):
    def test_buy_owner_is_the_receipt_recipient_not_lock_sender(self):
        call = {"sender_id": "lock.near", "msg": json.dumps({"Swap": {"swap_out_recipient": "creator.near"}})}
        receipt = {"predecessor_id": "wrap.near", "receipt": {"Action": {"signer_id": "creator.near", "actions": [{"FunctionCall": {"method_name": "ft_on_transfer", "args": base64.b64encode(json.dumps(call).encode()).decode()}}]}}}
        original = replay.rpc
        replay.rpc = lambda method, params: receipt
        try:
            self.assertEqual(replay.principal_from_receipt({"receipt": "r", "side": "buy"}, "coin.near"), ("creator.near", "VERIFIED"))
        finally:
            replay.rpc = original

    def test_rejects_receipt_without_ownership_proof(self):
        call = {"sender_id": "lock.near", "msg": json.dumps({"Swap": {"swap_out_recipient": "creator.near"}})}
        receipt = {"predecessor_id": "wrap.near", "receipt": {"Action": {"signer_id": "other.near", "actions": [{"FunctionCall": {"method_name": "ft_on_transfer", "args": base64.b64encode(json.dumps(call).encode()).decode()}}]}}}
        original = replay.rpc; replay.rpc = lambda method, params: receipt
        try:
            owner, status = replay.principal_from_receipt({"receipt": "r", "side": "buy"}, "coin.near")
            self.assertIsNone(owner); self.assertIn("UNKNOWN_OWNER", status)
        finally:
            replay.rpc = original

    def test_unattributed_sell_makes_measure_two_shares_unknown(self):
        coin = {"token": "coin.near", "creator": "creator.near", "launch_time_ms": 0}
        trades = [{"principal": "other.near", "side": "sell", "token_amount": "10", "ts_ms": 1, "block": 1},
                  {"principal": None, "side": "sell", "token_amount": "90", "ts_ms": 1, "block": 1}]
        row = replay.measure_2(coin, trades, 1, 1, 1_000)[0]
        self.assertEqual(row["everyone_else_share"], "UNKNOWN")
        self.assertEqual(row["unattributed_gross_sold_tokens"], 90)

    def test_fast_seller_share_uses_early_supply_not_total_supply(self):
        coin = {"token": "coin.near", "launch_time_ms": 0}
        trades = [{"principal": "fast.near", "side": "buy", "token_amount": "100", "ts_ms": 0, "block": 1},
                  {"principal": "fast.near", "side": "sell", "token_amount": "90", "ts_ms": 1, "block": 2},
                  {"principal": "slow.near", "side": "buy", "token_amount": "50", "ts_ms": 0, "block": 1}]
        row = replay.measure_3(coin, trades, 1, 1, 10_000)[0]
        self.assertEqual(row["fast_early_buyer_share_early_supply"], Decimal(2) / 3)


class SimulationTests(unittest.TestCase):
    def policy(self): return sim.Policy(17, .02, None, .1)

    def test_pool_cannot_pay_more_real_near_than_it_has(self):
        pool = sim.Pool(); pool.buy(5, 0); payout, _, _ = pool.sell(sim.SUPPLY, 0)
        self.assertLessEqual(payout, 5); self.assertGreaterEqual(pool.real, 0)

    def test_marked_sellers_share_their_allowance_pro_rata(self):
        engine = sim.Engine(self.policy()); engine.buy("a", 5, 0, True, 0); engine.buy("b", 5, 0, True, 0)
        marked_a, marked_b = engine.wallet("a").marked, engine.wallet("b").marked
        engine.cap_counted["a", engine.cap_bucket(1)] = -1_000_000  # isolate allowance from the cap
        engine.cap_counted["b", engine.cap_bucket(1)] = -1_000_000
        sold_a, _ = engine.sell("a", engine.wallet("a").tokens, 1)
        sold_b, _ = engine.sell("b", engine.wallet("b").tokens, 1)
        self.assertAlmostEqual(sold_a / marked_a, sold_b / marked_b, delta=1e-12)
        self.assertLessEqual(sold_a + sold_b, sim.ALLOWANCE * engine.pool.token * 1.001)

    def test_large_wallet_cannot_split(self):
        engine = sim.seed(self.policy()); wallet = engine.wallet("large-0")
        self.assertTrue(wallet.ever_large); self.assertEqual(engine.split("large-0", 5), [])

    def test_cap_uses_one_shared_unmarked_window(self):
        engine = sim.seed(self.policy()); engine.sell("creator", engine.wallet("creator").tokens, 96)
        self.assertIn(("creator", engine.cap_bucket(96)), engine.cap_counted)
        self.assertNotIn(("creator", engine.bucket(96, True)), engine.cap_counted)

    def test_unmarked_buy_restores_token_allowance(self):
        e = sim.Engine(self.policy()); e.buy("band", 5, 30)
        first, _ = e.sell("band", sim.ALLOWANCE * e.pool.token, 30)
        e.buy("band", 1, 30)
        second, _ = e.sell("band", sim.ALLOWANCE * e.pool.token, 30)
        self.assertGreater(first, 0); self.assertGreater(second, 0)

    def test_marked_sell_is_refused_through_n(self):
        e = sim.Engine(self.policy()); e.buy("early", 1, 0, True, 0)
        sold, _ = e.sell("early", e.wallet("early").tokens, 0, 17)
        self.assertEqual(sold, 0)

    def test_creator_group_cap_refuses_overflow(self):
        e = sim.Engine(self.policy()); e.buy("creator", 4, 0, True, 0, True)
        self.assertEqual(e.buy("creator.child", 1_000, 1, creator=True), 0)

    def test_post_sell_cap_uses_real_near_left(self):
        e = sim.Engine(self.policy(), sim.Pool(near=110, real=10)); w = e.wallet("large")
        w.tokens = sim.SUPPLY / 2; w.ever_large = True
        e.sell("large", w.tokens, 1)
        self.assertLessEqual(e.cap_used(1), sim.CAP * e.pool.real + sim.FLOOR * sim.SUPPLY * .000001)

    def test_buy_does_not_offset_another_accounts_cap(self):
        e = sim.Engine(self.policy(), sim.Pool(near=110, real=10)); e.buy("buyer", 100, 1)
        e.wallet("seller").tokens = sim.SUPPLY / 2; e.wallet("seller").ever_large = True
        e.sell("seller", e.wallet("seller").tokens, 1)
        self.assertGreater(e.cap_used(1), 0)
        self.assertEqual(e.account_cap("buyer", 1), 0)

    def test_band_penalty_only_applies_to_excess(self):
        e = sim.Engine(sim.Policy(17, .05, None, .1), sim.Pool(near=110, real=10))
        e.wallet("band").tokens = 30_000_000
        allowance=sim.ALLOWANCE*e.reserve_snapshot(1)
        e.sell("band", allowance, 1)
        self.assertEqual(e.band_penalties,0)
        requested=allowance;old_token,old_near=e.pool.token,e.pool.near;filled,_=e.sell("band",requested,1)
        expected=(old_near-old_token*old_near/(old_token+filled))*(1-sim.FEE)*.1
        self.assertAlmostEqual(e.band_penalties,expected,places=9)

    def test_sell_fee_is_charged_and_split(self):
        pool = sim.Pool(near=110, real=10)
        _, fee0, _ = pool.sell(1_000_000, 0)
        pool = sim.Pool(near=110, real=10)
        _, fee30, _ = pool.sell(1_000_000, .3)
        self.assertGreater(fee0, 0)
        self.assertAlmostEqual(fee30, fee0 * .7)

    def test_brake_trips_on_drop(self):
        e = sim.Engine(sim.Policy(17, .5, .1, .1), sim.Pool(near=110, real=10))
        e.wallet("band").tokens = 20_000_000
        e.prices[e.cap_bucket(1)] = e.price() * 2
        e.sell("band", 20_000_000, 1)
        self.assertGreater(e.brakes, 0)

    def test_full_cap_limits_large_seller_to_floor(self):
        e = sim.Engine(self.policy(), sim.Pool(near=110, real=10)); w=e.wallet("large")
        w.tokens=sim.SUPPLY/2;w.ever_large=True
        e.cap_counted["other",e.cap_bucket(1)]=1_000
        filled,_=e.sell("large",w.tokens,1)
        self.assertLessEqual(filled, sim.FLOOR*sim.SUPPLY+1e-6)

    def test_floor_is_one_account_allowance_per_window(self):
        e=sim.Engine(self.policy(),sim.Pool(near=110,real=10));w=e.wallet('big');w.tokens=sim.SUPPLY/2;w.ever_large=True
        e.cap_counted['other',e.cap_bucket(1)]=1e6
        first,_=e.sell('big',w.tokens,1);second,_=e.sell('big',w.tokens,1)
        self.assertAlmostEqual(first,sim.FLOOR*e.reserve_snapshot(1),places=6)
        self.assertEqual(second,0)

    def test_early_buy_fee_schedule_and_first_action_reserve_snapshot(self):
        for block,fee in ((0,.50),(17,.01+.49/18),(18,.01)):
            e=sim.Engine(self.policy()); out=e.buy('holder',1,0,block=block)
            expected=sim.SUPPLY-sim.SUPPLY*100/(100+1*(1-fee))
            self.assertAlmostEqual(out,expected,places=7)
        e=sim.Engine(self.policy());e.buy('a',1,1);snap=e.reserve_snapshot(1);e.buy('b',1,1)
        self.assertEqual(snap,sim.SUPPLY)

    def test_cap_cases_have_explicit_state_and_no_refusal_code(self):
        sim.cap_cases();data=json.loads((ROOT/'analysis/simulation/results/cap-cases.json').read_text())
        self.assertIn('early-buy-block-17',{c['name'] for c in data['cases']})
        band=next(c for c in data['cases'] if c['name']=='threshold-cross-at-2-percent')
        self.assertEqual(int(band['start_state']['holders'][0]['tokens_base']),20_000_000*10**18)
        self.assertFalse(band['start_state']['holders'][0]['large'])
        self.assertTrue(band['actions'][0]['result']['large_after'])
        band_sell=next(c for c in data['cases'] if c['name']=='band-sell-above-allowance')
        self.assertGreater(int(band_sell['actions'][1]['result']['penalty_near']),0)
        full=next(c for c in data['cases'] if c['name']=='full-cap-floor-and-second-sell')
        self.assertEqual(full['actions'][0]['result']['limit_reason'],'floor')
        self.assertEqual(full['actions'][1]['result']['fill'],'none')
        marked=next(c for c in data['cases'] if c['name']=='marked-allowance-limit')
        self.assertEqual(marked['actions'][-1]['result']['limit_reason'],'allowance')
        for case in data['cases']:
            state=case['start_state'];self.assertIn('holders',state)
            self.assertEqual(int(state['token_reserve_base'])+sum(int(h['tokens_base']) for h in state['holders']),int(state['total_supply_base']))
            for action in case['actions']:
                if action['kind']=='sell':
                    self.assertIn(action['result']['fill'],{'full','partial','none'})
                    self.assertIn(action['result']['limit_reason'],{None,'allowance','cap_full','floor'})
                    self.assertNotIn('refusal_code',action['result'])

    def test_runs_are_deterministic(self):
        a, b = sim.normal(self.policy()), sim.normal(self.policy())
        self.assertEqual(a.brakes, b.brakes); self.assertAlmostEqual(a.price(), b.price())


if __name__ == "__main__": unittest.main()
