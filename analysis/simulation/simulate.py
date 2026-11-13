#!/usr/bin/env python3
"""Deterministic synthetic exercise of the Issue #2 sell rules."""
from __future__ import annotations
import argparse, csv, json, random, math
from collections import defaultdict
from dataclasses import dataclass, field
from decimal import Decimal, ROUND_HALF_UP
from pathlib import Path

ROOT = Path(__file__).parent
SUPPLY, VIRTUAL, FEE, CAP, FLOOR = 1e9, 100., .01, .05, .001
ALLOWANCE = 1 / math.sqrt(1 - .02) - 1

@dataclass(frozen=True)
class Policy:
    n: int; threshold: float; brake: float | None; penalty: float; creator_fee_share: float = 0.; window: float = 1.

@dataclass
class Pool:
    token: float = SUPPLY; near: float = VIRTUAL; real: float = 0.; creator_fees: float = 0.
    def price(self): return self.near / self.token
    def gross(self, tokens): return max(0., self.near - self.token * self.near / (self.token + tokens))
    def buy(self, near, share, extra=0.):
        creator = near * FEE * share; effective = near * (1 - FEE - extra)
        out = self.token - self.token * self.near / (self.near + effective)
        self.token -= out; self.near += near - creator; self.real += near - creator; self.creator_fees += creator
        return out
    def sell(self, tokens, share, penalty=0.):
        gross = min(self.gross(tokens), self.real); normal = gross * FEE; creator = normal * share
        payout = max(0., gross - normal - gross * penalty)
        self.token += tokens; self.near -= payout + creator; self.real -= payout + creator; self.creator_fees += creator
        return payout, normal - creator + gross * penalty, gross * penalty
    def sell_band(self, tokens, first_tokens, share, penalty_rate):
        gross=min(self.gross(tokens),self.real);first=min(tokens,first_tokens);first_gross=min(self.gross(first),self.real)
        normal=gross*FEE;creator=normal*share;excess_after_fee=max(0.,gross-first_gross)*(1-FEE)
        penalty=excess_after_fee*penalty_rate;payout=max(0.,gross-normal-penalty)
        self.token+=tokens;self.near-=payout+creator;self.real-=payout+creator;self.creator_fees+=creator
        return payout,normal-creator+penalty,penalty,max(0.,payout-first_gross*(1-FEE))
    def tokens_for(self, payout, share, penalty=0.):
        lo, hi = 0., self.token
        for _ in range(100):
            mid = (lo + hi) / 2
            if min(self.gross(mid), self.real) * (1 - FEE - penalty) <= payout: lo = mid
            else: hi = mid
        return lo

@dataclass
class Wallet:
    name: str; tokens: float = 0.; marked: float = 0.; creator: bool = False; ever_large: bool = False; received: float = 0.

@dataclass
class Engine:
    policy: Policy; pool: Pool = field(default_factory=Pool); wallets: dict = field(default_factory=dict)
    sold: dict = field(default_factory=lambda: defaultdict(float)); bought: dict = field(default_factory=lambda: defaultdict(float))
    cap_counted: dict = field(default_factory=lambda: defaultdict(float)); cap_noncounted: dict = field(default_factory=lambda: defaultdict(float)); cap_buy: dict = field(default_factory=lambda: defaultdict(float)); floor_sold: dict = field(default_factory=lambda: defaultdict(float))
    snaps: dict = field(default_factory=dict); reserve_snaps: dict = field(default_factory=dict); prices: dict = field(default_factory=dict); brakes: int = 0; penalties: float = 0.; band_penalties: float = 0.; max_drawdown: float = 0.
    def __post_init__(self): self.initial_price = self.pool.price()
    def wallet(self, name, creator=False):
        w = self.wallets.setdefault(name, Wallet(name)); w.creator |= creator; return w
    def bucket(self, hour, marked=False): return int(hour // (24 if marked else self.policy.window))
    def cap_bucket(self, hour): return int(hour // self.policy.window)
    def price(self): return self.pool.price()
    def creator_tokens(self): return sum(w.tokens for w in self.wallets.values() if w.creator)
    def snapshot(self, hour, block):
        if block <= self.policy.n: return
        k = self.bucket(hour, True)
        if k not in self.snaps:
            amounts = {n:w.marked for n,w in self.wallets.items() if w.marked}; self.snaps[k] = amounts, sum(amounts.values()), self.pool.token
    def reserve_snapshot(self, hour):
        return self.reserve_snaps.setdefault(self.bucket(hour), self.pool.token)
    def buy(self, name, near, hour, early=False, block=10_000, creator=False):
        self.snapshot(hour, block); self.reserve_snapshot(hour); marked = block <= self.policy.n or creator or self.wallet(name).creator
        b=max(0,block)
        extra=.49*max(0.,18-min(18,b))/18 if b < 18 else 0.
        estimate = self.pool.token - self.pool.token * self.pool.near / (self.pool.near + near * (1-FEE-extra))
        group = creator or self.wallet(name).creator
        if group and self.creator_tokens() + estimate > .04 * SUPPLY: return 0.
        out = self.pool.buy(near, self.policy.creator_fee_share, extra); w = self.wallet(name, group)
        w.tokens += out; w.marked += out if marked or group else 0.; w.ever_large |= w.tokens >= self.policy.threshold * SUPPLY
        self.bought[name, self.bucket(hour)] += out
        self.cap_buy[name, self.cap_bucket(hour)] += near
        return out
    def transfer(self, source, target, amount, block, target_creator=False):
        a,b = self.wallet(source),self.wallet(target,target_creator)
        if amount > a.tokens or (b.creator and self.creator_tokens() + amount > .04*SUPPLY): return False
        marked=min(amount,a.marked); a.tokens-=amount; a.marked-=marked; b.tokens+=amount; b.marked+=amount if b.creator else marked
        b.ever_large |= b.tokens >= self.policy.threshold*SUPPLY; return True
    def account_cap(self, name, hour, counted=0.):
        k=(name,self.cap_bucket(hour)); buys=self.cap_buy[k]; noncounted=self.cap_noncounted[k]
        return max(0., self.cap_counted[k] + counted - max(0., buys - noncounted))
    def cap_used(self, hour, adding_name=None, adding=0.):
        names={n for n,k in self.cap_counted if k==self.cap_bucket(hour)} | {n for n,k in self.cap_buy if k==self.cap_bucket(hour)}
        return sum(self.account_cap(n,hour,adding if n==adding_name else 0.) for n in names | ({adding_name} if adding_name else set()))
    def cap_fill(self, name, wanted, hour, penalty):
        """Post-sell cap: used + payout <= c * real_after."""
        gross=min(self.pool.gross(wanted),self.pool.real); payout=gross*(1-FEE-penalty); creator=gross*FEE*self.policy.creator_fee_share
        if self.cap_used(hour,name,payout) <= CAP*(self.pool.real-payout-creator): return wanted
        lo,hi=0.,wanted
        for _ in range(100):
            mid=(lo+hi)/2; gross=min(self.pool.gross(mid),self.pool.real); payout=gross*(1-FEE-penalty); creator=gross*FEE*self.policy.creator_fee_share
            if self.cap_used(hour,name,payout) <= CAP*(self.pool.real-payout-creator): lo=mid
            else: hi=mid
        return lo
    def cap_fill_band(self,name,wanted,first,hour):
        def fits(total):
            gross=min(self.pool.gross(total),self.pool.real);first_gross=min(self.pool.gross(min(total,first)),self.pool.real)
            penalty=max(0.,gross-first_gross)*(1-FEE)*self.policy.penalty
            counted=max(0.,gross*(1-FEE)-penalty-first_gross*(1-FEE))
            creator=gross*FEE*self.policy.creator_fee_share
            return self.cap_used(hour,name,counted)<=CAP*(self.pool.real-gross*(1-FEE)-penalty-creator)
        if fits(wanted):return wanted
        lo,hi=min(wanted,first),wanted
        if hi<=lo:return lo
        for _ in range(100):
            mid=(lo+hi)/2
            if fits(mid):lo=mid
            else:hi=mid
        return lo
    def floor_tokens(self, name, hour, marked):
        reserve=self.reserve_snapshot(hour); key=(name,self.bucket(hour,marked))
        if marked:
            amounts,total,reserve=self.snaps[self.bucket(hour,True)]; allowance=FLOOR*reserve*amounts.get(name,0.)/total if total else 0.
        else: allowance=FLOOR*reserve
        return max(0.,allowance-self.floor_sold[key])
    def sell(self, name, wanted, hour, block=10_000):
        self.snapshot(hour, block); w=self.wallet(name); wanted=min(wanted,w.tokens); marked=min(wanted,w.marked); unmarked=wanted-marked
        if block<=self.policy.n: marked=0.
        total=payout=0.
        if marked:
            k=self.bucket(hour,True); amounts,supply,reserve=self.snaps[k]; allowance=ALLOWANCE*reserve*amounts.get(name,0.)/supply if supply else 0.
            wanted_marked=min(marked,max(0.,allowance-self.sold[name,k])); amount=self.cap_fill(name,wanted_marked,hour,0.)
            floor_used=0.
            if amount < wanted_marked:
                floor_used=min(wanted_marked-amount,self.floor_tokens(name,hour,True));amount+=floor_used
            paid,fee,_=self.pool.sell(amount,self.policy.creator_fee_share); self.sold[name,k]+=amount; self.cap_counted[name,self.cap_bucket(hour)]+=paid; self.penalties+=fee; total+=amount;payout+=paid
            if floor_used:self.floor_sold[name,self.bucket(hour,True)]+=floor_used
        if unmarked:
            k,ck=self.bucket(hour),self.cap_bucket(hour); room=max(0.,ALLOWANCE*self.reserve_snapshot(hour)+self.bought[name,k]-self.sold[name,k])
            base=min(unmarked,room); extra=0. if w.ever_large else max(0.,unmarked-base)
            start=self.prices.setdefault(ck,self.price())
            if extra and self.policy.brake is not None and 1-self.price()/start>=self.policy.brake: self.brakes+=1;extra=0.
            if w.ever_large:
                wanted_base=base;base=self.cap_fill(name,base,hour,0.);floor_used=0.
                if base<wanted_base:
                    floor_used=min(wanted_base-base,self.floor_tokens(name,hour,False));base+=floor_used
                if base:
                    paid,fee,_=self.pool.sell(base,self.policy.creator_fee_share);self.cap_counted[name,ck]+=paid;self.sold[name,k]+=base;self.penalties+=fee;total+=base;payout+=paid
                    if floor_used:self.floor_sold[name,ck]+=floor_used
            else:
                total_sell=base+extra
                if total_sell:
                    allowed=self.cap_fill_band(name,total_sell,base,hour)
                    # The base allowance always fills; only above-allowance tokens need cap room.
                    allowed=max(min(base,total_sell),allowed)
                    paid,fee,band,counted=self.pool.sell_band(allowed,base,self.policy.creator_fee_share,self.policy.penalty)
                    self.sold[name,k]+=allowed;self.cap_noncounted[name,ck]+=max(0.,paid-counted);self.cap_counted[name,ck]+=counted
                    self.penalties+=fee;self.band_penalties+=band;total+=allowed;payout+=paid
        w.tokens-=total;w.marked-=min(w.marked,total);w.received+=payout
        if w.tokens == 0:w.ever_large=False
        return total,payout
    def split(self, source,count):
        w=self.wallet(source)
        if w.ever_large:return []
        names=[f'{source}-{i}' for i in range(count)];each=w.tokens/count
        for n in names:self.wallet(n).tokens+=each
        w.tokens=0.;return names

def seed(policy, early_near=3.):
    e=Engine(policy)
    creator_near=(e.pool.token*e.pool.near/(e.pool.token-.04*SUPPLY)-e.pool.near)/(1-FEE-.49)
    e.buy('creator',creator_near,0,True,0,True)
    for i in range(5):e.buy(f'early-{i}',early_near,0,True,i*3)
    e.buy('small-early',.05,0,True,3)
    for i in range(3):e.buy(f'large-{i}',35,48+i)
    for i in range(8):e.buy(f'band-{i}',4,72+i)
    return e

def normal(policy):
    rng,e,high=random.Random(7),seed(policy),0.
    tick=1/6 if policy.window < 1 else 1
    for step in range(int(1440/tick)):
        h=step*tick
        for i in range(3):
            n=f'small-{i}'
            if rng.random()<.58 or not e.wallet(n).tokens:e.buy(n,rng.uniform(.03,.35),h)
            else:e.sell(n,e.wallet(n).tokens*rng.uniform(.05,.35),h)
        if step % int(6/tick)==0:e.sell(f'band-{(step//int(6/tick))%8}',e.wallet(f'band-{(step//int(6/tick))%8}').tokens*.4,h)
        if step % int(24/tick)==0:e.sell(f'large-{(step//int(24/tick))%3}',e.wallet(f'large-{(step//int(24/tick))%3}').tokens,h)
        high=max(high,e.price());e.max_drawdown=max(e.max_drawdown,1-e.price()/high)
    return e

def race(policy,reverse=False):
    e=seed(policy);names=['large-1','large-0'] if reverse else ['large-0','large-1'];a=e.sell(names[0],e.wallet(names[0]).tokens,96);b=e.sell(names[1],e.wallet(names[1]).tokens,96)
    return {'first_tokens':a[0],'first_near':a[1],'second_tokens':b[0],'second_near':b[1],'cap_used_near':e.cap_used(96)}
def cap_race(policy):
    e=seed(policy); fills=[e.sell(f'large-{i}',e.wallet(f'large-{i}').tokens,96) for i in range(3)]
    return {'seller_count':3,'first_near':fills[0][1],'second_near':fills[1][1],'third_near':fills[2][1],'cap_used_near':e.cap_used(96)}
def split_test(p):
    a,b=seed(p),seed(p);x=a.sell('band-0',a.wallet('band-0').tokens,100);kids=b.split('band-0',5);y=sum(b.sell(k,b.wallet(k).tokens,100)[1] for k in kids)
    return {'direct_tokens':x[0],'direct_near':x[1],'split_near':y,'extra_near':y-x[1],'split_cost_near':.035}
def round_trip(p,replenish=False):
    e=seed(p);x=e.buy('attacker',12,100)
    if replenish:e.buy('replenisher',30,100)
    sold,got=e.sell('attacker',x,100);return {'near_spent':12.,'near_received':got,'net':got-12,'sold_tokens':sold,'cap_used_near':e.cap_used(100)}
def creator_exit(p,snipers=False):
    e=seed(p);start=e.wallet('creator').tokens
    for h in range(1440):
        if snipers and h%8==0:e.buy(f'sniper-{h}',1,h)
        e.sell('creator',e.wallet('creator').tokens,h)
        if e.wallet('creator').tokens<1:break
    return {'start_tokens':start,'remaining_tokens':e.wallet('creator').tokens,'days':h/24,'exit_status':'out' if e.wallet('creator').tokens<1 else 'not out by day 60'}
def early_exit(p,share):
    e=seed(p);total=sum(w.marked for w in e.wallets.values());f=share*SUPPLY/total
    for w in e.wallets.values():
        if w.marked:w.marked*=f;w.tokens=w.marked
    w=e.wallet('early-0')
    for h in range(1440):
        e.sell(w.name,w.tokens,h)
        if w.marked<1:break
    return {'early_supply_share':share,'holder_days':h/24,'remaining_marked_tokens':w.marked,'exit_status':'out' if w.marked<1 else 'not out by day 60'}
def base(p):return {'label':'simulation','n':p.n,'t':p.threshold,'brake':'off' if p.brake is None else p.brake,'penalty':p.penalty,'creator_fee_share':p.creator_fee_share,'window_hours':p.window}
def rows(p,only=None):
    b=base(p);e=normal(p);r={1:dict(b,test='1_normal_brake',brake_trips=e.brakes,max_drawdown=e.max_drawdown,final_price=e.price()),2:dict(b,test='2_first_seller_race',**race(p)),3:dict(b,test='3_cap_race_many_sellers',**cap_race(p)),4:dict(b,test='4_small_holder_drawdown',max_drawdown=e.max_drawdown,final_price=e.price(),penalty_near=e.band_penalties),5:dict(b,test='5_brake_penalty',brakes=e.brakes,penalty_near=e.band_penalties),6:dict(b,test='6_penalty_level',penalty_near=e.band_penalties),7:dict(b,test='7_band_split',**split_test(p)),8:dict(b,test='8_cap_round_trip',**round_trip(p)),9:dict(b,test='9_buy_raise_cap',**round_trip(p,True))}
    if not only or 10 in only:
        r[10]=dict(b,test='10_creator_exit',**creator_exit(p,False));r[10].update({f'sniper_{k}':v for k,v in creator_exit(p,True).items()})
    out=[r[i] for i in sorted(only or r) if i in r]
    return out+([dict(b,test='11_early_supply_exit',**early_exit(p,x)) for x in (.09,.23,.41)] if not only or 11 in only else [])
def write(name,rows,directory=None):
    cols=list(dict.fromkeys(k for r in rows for k in r));path=(directory or ROOT/'results')/name;path.parent.mkdir(parents=True,exist_ok=True)
    with path.open('w',newline='') as f:o=csv.DictWriter(f,fieldnames=cols,lineterminator='\n');o.writeheader();o.writerows(rows)
def unit(value, scale): return str(int((Decimal(str(value))*Decimal(str(scale))).quantize(Decimal('1'),rounding=ROUND_HALF_UP)))
def cap_cases():
    """Export reproducible examples, including each launch-fee block."""
    cases=[]
    def start(e):
        holders=[{'account':w.name,'tokens_base':unit(w.tokens,1e18),'marked_base':unit(w.marked,1e18),'large':w.ever_large} for w in e.wallets.values()]
        reserve_base=int(unit(SUPPLY,1e18))-sum(int(w['tokens_base']) for w in holders)
        return {'real_near_yocto':unit(e.pool.real,1e24),'virtual_near_yocto':unit(e.pool.near-e.pool.real,1e24),
                'token_reserve_base':str(reserve_base),'holders':holders,'total_supply_base':unit(SUPPLY,1e18),
                'rules':{'n':e.policy.n,'t':e.policy.threshold,'penalty':e.policy.penalty,'creator_fee_share_bps':int(e.policy.creator_fee_share*10000),
                         'cap_bps':int(CAP*10000),'floor_bps':int(FLOOR*10000),'window_s':int(e.policy.window*3600),'allowance':ALLOWANCE}}
    def case(name,state,actions):cases.append({'name':name,'start_state':state,'actions':actions})
    def buy(e,actions,name,near,block,second=0,creator=False):
        h=second/3600;before=e.wallet(name).ever_large;out=e.buy(name,near,h,block=block,creator=creator)
        actions.append({'account':name,'kind':'buy','amount_near_yocto':unit(near,1e24),'time_s':second,'block':block,'result':{'tokens_filled_base':unit(out,1e18),'large_before':before,'large_after':e.wallet(name).ever_large,'cap_used_yocto':unit(e.cap_used(h),1e24)}})
    def sell(e,actions,name,amount,second=0,block=100):
        h=second/3600;e.snapshot(h,block);w=e.wallet(name);marked=min(amount,w.marked);pre_cap=e.cap_used(h);pre_limit=CAP*e.pool.real
        allowance=ALLOWANCE*e.reserve_snapshot(h)
        if marked: allowance=ALLOWANCE*e.snaps[e.bucket(h,True)][2]*marked/max(e.snaps[e.bucket(h,True)][1],1)
        prior_penalty=e.band_penalties;filled,paid=e.sell(name,amount,h,block);reason=None
        if filled<amount:
            if pre_cap>=pre_limit-1e-9 and filled<=FLOOR*e.reserve_snapshot(h)+1e-6:reason='floor'
            elif pre_cap>=pre_limit-1e-9:reason='cap_full'
            elif filled>=allowance-1e-6:reason='allowance'
            else:reason='cap_full'
        actions.append({'account':name,'kind':'sell','amount_token_base':unit(amount,1e18),'time_s':second,'block':block,
                        'result':{'tokens_filled_base':unit(filled,1e18),'near_out_yocto':unit(paid,1e24),'fill':'full' if filled>=amount-1e-6 else ('none' if filled<=1e-9 else 'partial'),'limit_reason':reason,'penalty_near':unit(e.band_penalties-prior_penalty,1e24),'cap_used_yocto':unit(e.cap_used(h),1e24)}})
    p=Policy(17,.02,None,.1)
    # The band holder starts at 2%; a buy crosses the 5% threshold.
    band_policy=Policy(17,.05,None,.1);e=Engine(band_policy,Pool(token=SUPPLY-20_000_000.,near=100.,real=0.));e.wallet('band').tokens=20_000_000.;state=start(e);actions=[];buy(e,actions,'band',4.,100);sell(e,actions,'band',e.wallet('band').tokens*.9,1);case('threshold-cross-at-2-percent',state,actions)
    e=Engine(band_policy,Pool(token=SUPPLY-20_000_000.,near=105.,real=5.));e.wallet('band').tokens=20_000_000.;state=start(e);actions=[];buy(e,actions,'band',.1,100);sell(e,actions,'band',e.wallet('band').tokens,1);case('band-sell-above-allowance',state,actions)
    # Each launch-relative block has an explicit buy vector.
    for b in range(18):
        e=Engine(p,Pool(token=SUPPLY,near=100.,real=0.));state=start(e);actions=[];buy(e,actions,'early-buyer',1.,b);case(f'early-buy-block-{b}',state,actions)
    e=Engine(p,Pool(token=SUPPLY,near=100.,real=0.));state=start(e);actions=[];buy(e,actions,'marked',1.,0);buy(e,actions,'other-marked',9.,0);sell(e,actions,'marked',e.wallet('marked').tokens,1,18);case('marked-allowance-limit',state,actions)
    e=Engine(p,Pool(token=SUPPLY,near=110.,real=10.));setup=[];buy(e,setup,'large',3.,100,3600);state=start(e);state['cap_used_by_account_yocto']={'other':unit(CAP*e.pool.real,1e24)};e.cap_counted['other',1]=CAP*e.pool.real;actions=[];sell(e,actions,'large',e.wallet('large').tokens,3601);sell(e,actions,'large',e.wallet('large').tokens,3602);case('full-cap-floor-and-second-sell',state,actions);cases[-1]['state_after_setup']=True;cases[-1]['setup_actions']=setup
    (ROOT/'results'/'cap-cases.json').write_text(json.dumps({'version':'1.0.0-rc.5','math':'binary floating point; 100 bisection iterations; serialized values rounded to nearest base unit','cases':cases},indent=2)+'\n')
def main():
    a=argparse.ArgumentParser();a.add_argument('--checkpoint',action='store_true');a.add_argument('--shard',type=int);a.add_argument('--shards',type=int);a.add_argument('--merge',action='store_true');a.add_argument('--cap-cases',action='store_true');args=a.parse_args()
    if args.cap_cases: cap_cases();return
    if args.merge:
        for i in range(1,12):
            records=[]
            for shard in range(args.shards or 1):
                path=ROOT/'results'/'shards'/str(shard)/f'test-{i}.csv'
                if path.exists(): records.extend(csv.DictReader(path.open()))
            write(f'test-{i}.csv',records)
        cap_cases();return
    if args.checkpoint: result=[r for s in (0.,.3) for r in rows(Policy(17,.02,None,.1,s),{1,4})]
    else:
        grid=[Policy(n,.02,d,pen,s) for n in (5,17,50) for d in (None,.1,.2,.3) for pen in (.1,.2,.3) for s in (0.,.3)]
        if args.shard is not None: grid=grid[args.shard::args.shards]
        result=[r for p in grid for r in rows(p,set(range(1,10)))]
        if args.shard in (None,0):
            for t in (.01,.05):result+=rows(Policy(17,t,None,.1),set(range(1,10)))
            for w in (1/6,24):result+=rows(Policy(17,.02,None,.1,0.,w),set(range(1,10)))
            for s in (0.,.3):result+=rows(Policy(17,.02,None,.1,s),{10,11})
    directory=ROOT/'results'/'checkpoint' if args.checkpoint else (ROOT/'results'/'shards'/str(args.shard) if args.shard is not None else ROOT/'results')
    for i in (1,4) if args.checkpoint else range(1,12):
        write(f'test-{i}.csv',[r for r in result if r['test'].startswith(f'{i}_')],directory)
    if not args.checkpoint and args.shard is None: cap_cases()
if __name__=='__main__':main()
