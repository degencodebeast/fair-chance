import base64
import importlib.util
import json
import sys
import tempfile
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
        engine.cap_net["a", engine.cap_bucket(1)] = -1_000_000  # isolate allowance from the cap
        engine.cap_net["b", engine.cap_bucket(1)] = -1_000_000
        sold_a, _ = engine.sell("a", engine.wallet("a").tokens, 1)
        sold_b, _ = engine.sell("b", engine.wallet("b").tokens, 1)
        self.assertGreater(sold_a, 0); self.assertGreater(sold_b, 0)
        total=marked_a+marked_b;reserve=engine.reserve_snapshot(1)
        self.assertLessEqual(sold_a, sim.ALLOWANCE*reserve*marked_a/total+1e-6)
        self.assertLessEqual(sold_b, sim.ALLOWANCE*reserve*marked_b/total+1e-6)

    def test_large_wallet_cannot_split(self):
        engine = sim.seed(self.policy()); wallet = engine.wallet("large-0")
        self.assertTrue(wallet.ever_large); self.assertEqual(engine.split("large-0", 5), [])

    def test_cap_uses_one_shared_unmarked_window(self):
        engine = sim.seed(self.policy()); engine.sell("creator", engine.wallet("creator").tokens, 96)
        self.assertIn(("creator", engine.cap_bucket(96)), engine.cap_net)
        self.assertNotIn(("creator", engine.bucket(96, True)), engine.cap_net)

    def test_unmarked_buy_restores_token_allowance(self):
        e = sim.Engine(self.policy()); e.buy("band", 5, 30)
        first, _ = e.sell("band", sim.ALLOWANCE * e.pool.token, 30)
        e.buy("band", 1, 30)
        second, _ = e.sell("band", sim.ALLOWANCE * e.pool.token, 30)
        self.assertGreater(first, 0); self.assertGreater(second, 0)

    def test_marked_sell_is_refused_through_n(self):
        e = sim.Engine(self.policy()); e.buy("early", 1, 0, True, 0)
        e.wallet("early").tokens += 1_000_000
        sold, _ = e.sell("early", e.wallet("early").tokens, 0, 17)
        self.assertEqual(sold, 0)
        self.assertEqual(e.last_refusal_code["early"], "FC_EARLY_WINDOW")
        self.assertEqual(e.wallet("early").tokens, e.wallet("early").marked+1_000_000)

    def test_creator_group_cap_refuses_overflow(self):
        e = sim.Engine(self.policy()); e.buy("creator", 4, 0, True, 0, True)
        self.assertEqual(e.buy("creator.child", 1_000, 1, creator=True), 0)

    def test_post_sell_cap_uses_real_near_left(self):
        e = sim.Engine(self.policy(), sim.Pool(near=110, real=10)); w = e.wallet("large")
        w.tokens = sim.SUPPLY / 2; w.ever_large = True
        e.sell("large", w.tokens, 1)
        self.assertLessEqual(e.cap_used(1), sim.CAP * e.pool.real + 1e-9)

    def test_rc7_cap_left_matches_exact_creator_fee_formula(self):
        e=sim.Engine(sim.Policy(17,.02,None,.1,.3),sim.Pool(near=110,real=10))
        e.cap_net['other',e.cap_bucket(1)]=.1
        c,f,s=sim.CAP,sim.FEE,e.policy.creator_fee_share
        expected=(c*e.pool.real-e.cap_used(1))*(1-f)/((1+c)*(1-f)+c*s*f)
        self.assertAlmostEqual(e.cap_left(1),expected,places=14)
        e.wallet('large').tokens=100_000_000;e.wallet('large').ever_large=True
        e.sell('large',10_000_000,1)
        expected=max(0,(c*e.pool.real-e.cap_used(1))*(1-f)/((1+c)*(1-f)+c*s*f))
        self.assertAlmostEqual(e.cap_left(1),expected,places=14)

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
        e.cap_net["other",e.cap_bucket(1)]=1_000
        filled,_=e.sell("large",w.tokens,1)
        self.assertLessEqual(filled, sim.FLOOR*sim.SUPPLY+1e-6)

    def test_floor_is_one_account_allowance_per_window(self):
        e=sim.Engine(self.policy(),sim.Pool(near=110,real=10));w=e.wallet('big');w.tokens=sim.SUPPLY/2;w.ever_large=True
        e.cap_net['other',e.cap_bucket(1)]=1e6
        first,_=e.sell('big',w.tokens,1);second,_=e.sell('big',w.tokens,1)
        self.assertAlmostEqual(first,sim.FLOOR*e.reserve_snapshot(1),places=6)
        self.assertEqual(second,0)

    def test_rc7_floor_never_refunds_buys_and_subtracts_marked_sales(self):
        e=sim.Engine(self.policy(),sim.Pool(near=110,real=10));w=e.wallet('big');w.tokens=sim.SUPPLY/2;w.ever_large=True
        e.cap_net['other',e.cap_bucket(1)]=1e6
        e.unmarked_sold['big',e.bucket(1)]=200_000
        e.unmarked_bought['big',e.bucket(1)]=100_000
        self.assertAlmostEqual(e.floor_tokens('big',1,False),sim.FLOOR*sim.SUPPLY-200_000,places=6)
        e.marked_sold_hour['big',e.cap_bucket(1)]=300_000
        self.assertAlmostEqual(e.floor_tokens('big',1,False),sim.FLOOR*sim.SUPPLY-500_000,places=6)

    def test_marked_sell_preserves_unmarked_balance(self):
        e=sim.Engine(self.policy(),sim.Pool(token=sim.SUPPLY,near=200,real=100));w=e.wallet('holder')
        w.tokens=10_000_000;w.marked=5_000_000
        e.reserve_snaps[0]=sim.SUPPLY
        e.snaps[0]=({'holder':5_000_000,'other':495_000_000},500_000_000,sim.SUPPLY)
        marked_fill=sim.ALLOWANCE*sim.SUPPLY*.01;unmarked_fill=5_000_000
        e.sell('holder',marked_fill+unmarked_fill,1)
        actual_marked=e.marked_sold['holder',e.bucket(1,True)]
        self.assertAlmostEqual(actual_marked,marked_fill,delta=1)
        self.assertAlmostEqual(w.marked,5_000_000-actual_marked,places=7)
        self.assertAlmostEqual(w.tokens,10_000_000-actual_marked-unmarked_fill,places=7)

    def test_small_sell_fills_after_receive(self):
        e=sim.Engine(self.policy(),sim.Pool(near=110,real=10));w=e.wallet('small');w.tokens=10_000_000
        first,_=e.sell('small',6_000_000,1);self.assertEqual(first,6_000_000)
        e.wallet('giver').tokens=5_000_000
        self.assertTrue(e.transfer('giver','small',5_000_000,100,hour=1))
        self.assertEqual(w.tokens,9_000_000)
        second,_=e.sell('small',9_000_000,1)
        self.assertEqual(second,9_000_000)

    def test_rc6_cap_net_counts_buys_once_and_marked_sells_gross(self):
        e=sim.Engine(self.policy(),sim.Pool(near=110,real=10))
        e.record_cap_buy('a',1,10);e.record_cap_sell('a',1,counted=5)
        self.assertEqual(e.account_cap('a',1),0)
        e.record_cap_sell('a',1,counted=0)
        self.assertEqual(e.account_cap('a',1),0)
        e.record_marked_sell('a',1,3)
        e.record_cap_buy('a',1,30)
        self.assertEqual(e.account_cap('a',1),3)
        e=sim.Engine(self.policy(),sim.Pool(near=110,real=10))
        e.record_cap_buy('a',1,30);e.record_cap_sell('a',1,counted=29.4108)
        self.assertEqual(e.account_cap('a',1),0)

    def test_rc6_small_old_tokens_are_uncapped_but_window_buys_are_counted(self):
        old=sim.Engine(self.policy(),sim.Pool(near=110,real=10));old.wallet('small').tokens=10_000_000
        old.cap_net['other',old.cap_bucket(1)]=sim.CAP*old.pool.real
        old_fill,_=old.sell('small',10_000_000,1)
        self.assertEqual(old_fill,10_000_000)
        bought=sim.Engine(self.policy(),sim.Pool(near=110,real=10));bought.wallet('small').tokens=10_000_000
        bought.cap_bought_tokens['small',bought.cap_bucket(1)]=10_000_000
        bought.cap_net['other',bought.cap_bucket(1)]=sim.CAP*bought.pool.real
        bought_fill,_=bought.sell('small',10_000_000,1)
        self.assertLess(bought_fill,10_000_000)

    def test_small_sell_payout_offsets_later_buy_before_large_sell(self):
        e=sim.Engine(self.policy(),sim.Pool(near=110,real=10));e.wallet('a').tokens=5_000_000
        e.sell('a',5_000_000,1);e.buy('a',30,1,block=100)
        self.assertTrue(e.wallet('a').ever_large)
        e.sell('a',e.wallet('a').tokens,1)
        self.assertEqual(e.account_cap('a',1),0)

    def test_marked_and_unmarked_sell_counters_do_not_collide(self):
        e=sim.Engine(self.policy(),sim.Pool(near=110,real=10));w=e.wallet('large');w.tokens=30_000_000;w.ever_large=True
        e.marked_sold['large',0]=30_000_000
        filled,_=e.sell('large',30_000_000,.5)
        self.assertGreater(filled,0)
        self.assertGreater(e.unmarked_sold['large',0],0)

    def test_band_class_floor_survives_first_below_allowance_sell(self):
        e=sim.Engine(sim.Policy(17,.05,None,.1),sim.Pool(near=110,real=10))
        e.wallet('band').tokens=12_000_000
        first,_=e.sell('band',5_000_000,1)
        self.assertEqual(first,5_000_000)
        self.assertEqual(e.live_class('band',1),'small')
        self.assertEqual(e.account_class('band',1),'band')
        before=e.band_penalties;near=e.pool.near;tokens=e.pool.token
        second,_=e.sell('band',7_000_000,1)
        self.assertEqual(second,7_000_000)
        gross=near-tokens*near/(tokens+second)
        second_allowance=max(0,sim.ALLOWANCE*e.reserve_snapshot(1)-5_000_000)
        first_gross=near-tokens*near/(tokens+second_allowance)
        expected=max(0,gross-first_gross)*(1-sim.FEE)*.1
        self.assertAlmostEqual(e.band_penalties-before,expected,places=8)

    def test_receive_can_raise_window_class(self):
        e=sim.Engine(sim.Policy(17,.05,None,.1));e.wallet('sender').tokens=2_000_000;e.wallet('receiver').tokens=9_800_000
        self.assertTrue(e.transfer('sender','receiver',500_000,100,hour=1))
        self.assertEqual(e.account_class('receiver',1),'band')

    def test_large_falls_to_band_only_after_its_balance_reaches_zero(self):
        e=sim.Engine(self.policy(),sim.Pool(near=110,real=10));w=e.wallet('large');w.tokens=20_000_000;w.ever_large=True
        e.cap_net['large',e.cap_bucket(1)]=-1e9
        e.unmarked_bought['large',e.bucket(1)]=sim.SUPPLY
        first,_=e.sell('large',w.tokens/2,1)
        self.assertGreater(first,0);self.assertEqual(e.account_class('large',1),'large')
        e.sell('large',w.tokens,1)
        self.assertEqual(w.tokens,0)
        self.assertFalse(w.ever_large)
        self.assertEqual(e.account_class('large',1),'band')

    def test_early_buy_fee_schedule_and_first_action_reserve_snapshot(self):
        for block,fee in ((0,.50),(17,.01+.49/18),(18,.01)):
            e=sim.Engine(self.policy()); out=e.buy('holder',1,0,block=block)
            expected=sim.SUPPLY-sim.SUPPLY*100/(100+1*(1-fee))
            self.assertAlmostEqual(out,expected,places=6)
            extra=.49*max(0.,18-min(18,block))/18 if block<18 else 0.
            effective=Decimal(1)*(Decimal(1)-Decimal('.01')-Decimal(str(extra)))
            exact_base=int((Decimal('1e27')*effective/(Decimal(100)+effective)).to_integral_value(rounding='ROUND_DOWN'))
            self.assertEqual(e.pool.last_buy_tokens_base,exact_base)
        e=sim.Engine(self.policy());e.buy('a',1,1);snap=e.reserve_snapshot(1);e.buy('b',1,1)
        self.assertEqual(snap,sim.SUPPLY)

    def test_cap_cases_have_explicit_state_and_refusals_only_for_early_marked_sells(self):
        with tempfile.TemporaryDirectory() as tmp:
            case_path=Path(tmp)/'cap-cases.json';sim.cap_cases(case_path)
            data=json.loads(case_path.read_text())
        self.assertIn('early-buy-block-17',{c['name'] for c in data['cases']})
        self.assertEqual(data['interface'],'1.0.0-rc.7')
        self.assertEqual(data['version'],'1.0.0-rc.7')
        band=next(c for c in data['cases'] if c['name']=='threshold-cross-at-2-percent')
        self.assertEqual(int(band['start_state']['holders'][0]['tokens_base']),20_000_000*10**18)
        self.assertFalse(band['start_state']['holders'][0]['large'])
        self.assertTrue(band['actions'][0]['result']['large_after'])
        self.assertIn('real NEAR',band['note'])
        band_sell=next(c for c in data['cases'] if c['name']=='band-sell-above-allowance')
        self.assertEqual(band_sell['actions'][1]['result']['fill'],'partial')
        self.assertEqual(band_sell['actions'][1]['result']['limit_reason'],'cap_full')
        self.assertEqual(int(band_sell['actions'][1]['result']['tokens_filled_base']),12_911_252_650_806_659_844_417_354)
        self.assertEqual(int(band_sell['actions'][1]['result']['penalty_near']),21_148_202_867_270_188_000_000)
        full=next(c for c in data['cases'] if c['name']=='full-cap-floor-and-second-sell')
        self.assertFalse(full['start_state']['holders'][0]['large'])
        self.assertTrue(full['actions'][0]['result']['large_after'])
        self.assertEqual(full['actions'][1]['result']['limit_reason'],'cap_full')
        self.assertEqual(full['actions'][-1]['result']['limit_reason'],'cap_full')
        band_cap=next(c for c in data['cases'] if c['name']=='band-sell-above-allowance')
        self.assertEqual(band_cap['actions'][-1]['result']['limit_reason'],'cap_full')
        self.assertEqual({c['name'] for c in data['cases'] if c['name'].startswith('post-sell-cap')},{'post-sell-cap-s-0','post-sell-cap-s-30'})
        self.assertEqual({c['name'] for c in data['cases'] if c['name'].startswith('same-account-round-trip')},{'same-account-round-trip-s-0','same-account-round-trip-s-30'})
        self.assertTrue(all(c['actions'][-1]['amount']=='all' and 'amount_token_base' not in c['actions'][-1] for c in data['cases'] if c['name'].startswith('same-account-round-trip')))
        self.assertEqual({c['name'] for c in data['cases'] if c['name'].startswith('order-independence')},{'order-independence-buy-first','order-independence-sell-first'})
        credit=next(c for c in data['cases'] if c['name']=='band-round-trip-credit')
        self.assertTrue(credit['actions'][-2]['result']['large_after'])
        self.assertEqual(data['cases'][0]['start_state']['rules']['p_bps'],200)
        self.assertEqual(data['cases'][0]['start_state']['rules']['swap_fee_bps'],100)
        self.assertEqual(data['cases'][0]['start_state']['rules']['early_fee_start_bps'],5000)
        marked=next(c for c in data['cases'] if c['name']=='marked-allowance-limit')
        self.assertEqual(marked['actions'][-1]['result']['limit_reason'],'allowance')
        class_floor=next(c for c in data['cases'] if c['name']=='band-class-floor-two-sells')
        self.assertEqual(int(class_floor['start_state']['holders'][0]['tokens_base']),12_000_000*10**18)
        self.assertEqual(class_floor['actions'][0]['result']['class_after'],'band')
        self.assertEqual(class_floor['actions'][1]['result']['class_before'],'band')
        self.assertEqual(class_floor['actions'][1]['result']['fill'],'full')
        self.assertEqual(int(class_floor['actions'][1]['result']['penalty_near']),21_231_060_362_107_722_000_000)
        floor_marked=next(c for c in data['cases'] if c['name']=='floor-marked-first')
        floor_unmarked=next(c for c in data['cases'] if c['name']=='floor-unmarked-first')
        for c in (floor_marked,floor_unmarked):
            floor=int(c['start_state']['reserve_snapshot_token_base'])//1000
            self.assertEqual(sum(int(a['result']['tokens_filled_base']) for a in c['actions']),floor)
            self.assertEqual(c['actions'][0]['result']['limit_reason'],'floor')
        early=next(c for c in data['cases'] if c['name']=='early-window-refuses-mixed-sell')
        self.assertEqual(early['actions'][0]['result']['refusal_code'],'FC_EARLY_WINDOW')
        self.assertEqual(early['actions'][0]['result']['fill'],'none')
        marked_limit=next(c for c in data['cases'] if c['name']=='marked-allowance-limit')
        self.assertEqual(marked_limit['actions'][-1]['amount'],'all')
        small_round=next(c for c in data['cases'] if c['name']=='small-buy-sell-at-full-cap')
        self.assertEqual(small_round['actions'][-1]['amount'],'all')
        resale=next(c for c in data['cases'] if c['name']=='marked-resale-two-account-cap')
        bought_tokens=int(resale['actions'][1]['result']['tokens_filled_base'])
        self.assertEqual(int(resale['actions'][2]['amount']),bought_tokens)
        one_swap=next(c for c in data['cases'] if c['name']=='marked-sale-reduces-unmarked-floor')
        self.assertEqual(int(one_swap['actions'][0]['result']['near_out_yocto']),108_791_208_791_208_800_000_000)
        tiny=next(c for c in data['cases'] if c['name']=='cap-left-s-30')
        self.assertIn(int(tiny['actions'][1]['result']['tokens_filled_base']),{16,17})
        self.assertEqual([a['result']['limit_reason'] for a in tiny['actions'][1:]],['cap_full','cap_full'])
        self.assertEqual(resale['actions'][0]['result']['limit_reason'],'allowance')
        self.assertEqual(resale['actions'][1]['result']['tokens_filled_base'],resale['actions'][2]['amount'])
        self.assertEqual(resale['actions'][2]['result']['tokens_filled_base'],resale['actions'][1]['result']['tokens_filled_base'])
        self.assertEqual(resale['actions'][2]['result']['fill'],'full')
        self.assertIsNone(resale['actions'][2]['result']['limit_reason'])
        self.assertEqual(resale['actions'][3]['result']['limit_reason'],'allowance')
        held=next(c for c in data['cases'] if c['name']=='marked-cap-hold-does-not-use-unmarked-floor')
        self.assertEqual(int(held['actions'][0]['result']['tokens_filled_base']),0)
        self.assertEqual(int(held['actions'][1]['result']['tokens_filled_base']),864_000_000_000_000_000_000_000)
        self.assertEqual(held['actions'][1]['result']['limit_reason'],'floor')
        cap_left=next(c for c in data['cases'] if c['name']=='cap-left-s-30')
        self.assertEqual(cap_left['start_state']['creator_fee_share_bps'],3000)
        self.assertTrue(all('cap_left_before_yocto' in a['result'] and 'cap_left_yocto' in a['result'] for a in cap_left['actions']))
        band_cap_left=next(c for c in data['cases'] if c['name']=='band-above-allowance-cap-left')['actions'][0]['result']
        self.assertLess(int(band_cap_left['counted_near_yocto']),int(band_cap_left['cap_left_before_yocto']))
        for name in ('marked-sell-buy-resell','marked-resale-two-account-cap','small-buy-sell-at-full-cap',
                     'small-buy-price-rise-cap','marked-and-unmarked-old-plus-buy-resell','marked-sale-reduces-unmarked-floor'):
            self.assertIn(name,{c['name'] for c in data['cases']})
        small_round=next(c for c in data['cases'] if c['name']=='small-buy-sell-at-full-cap')
        self.assertEqual(small_round['actions'][-1]['result']['fill'],'full')
        price_rise=next(c for c in data['cases'] if c['name']=='small-buy-price-rise-cap')
        self.assertEqual(price_rise['actions'][-1]['result']['fill'],'partial')
        self.assertEqual(price_rise['actions'][-1]['result']['limit_reason'],'cap_full')
        for case in data['cases']:
            state=case['start_state'];self.assertIn('holders',state)
            self.assertEqual(int(state['token_reserve_base'])+sum(int(h['tokens_base']) for h in state['holders']),int(state['total_supply_base']))
            for action in case['actions']:
                if action['kind']=='sell':
                    self.assertNotIn('amount_token_base',action)
                    self.assertIn(action['result']['fill'],{'full','partial','none'})
                    self.assertIn(action['result']['limit_reason'],{None,'allowance','cap_full','floor'})
                    if case['name']=='early-window-refuses-mixed-sell':
                        self.assertEqual(action['result']['refusal_code'],'FC_EARLY_WINDOW')
                        self.assertEqual(action['result']['fill'],'none')
                    else:self.assertNotIn('refusal_code',action['result'])
                    self.assertEqual(action['time_s'],int(action['time_s']))

    def test_runs_are_deterministic(self):
        a, b = sim.normal(self.policy()), sim.normal(self.policy())
        self.assertEqual(a.brakes, b.brakes); self.assertAlmostEqual(a.price(), b.price())


if __name__ == "__main__": unittest.main()
