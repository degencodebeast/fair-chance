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
    def sell_band(self, tokens, uncounted_tokens, penalty_free_tokens, share, penalty_rate):
        gross=min(self.gross(tokens),self.real);uncounted=min(tokens,uncounted_tokens);penalty_free=min(tokens,penalty_free_tokens)
        uncounted_gross=min(self.gross(uncounted),self.real);penalty_free_gross=min(self.gross(penalty_free),self.real)
        normal=gross*FEE;creator=normal*share;excess_after_fee=max(0.,gross-penalty_free_gross)*(1-FEE)
        penalty=excess_after_fee*penalty_rate;payout=max(0.,gross-normal-penalty)
        self.token+=tokens;self.near-=payout+creator;self.real-=payout+creator;self.creator_fees+=creator
        counted=max(0.,payout-uncounted_gross*(1-FEE))
        return payout,normal-creator+penalty,penalty,counted
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
    unmarked_sold: dict = field(default_factory=lambda: defaultdict(float)); unmarked_bought: dict = field(default_factory=lambda: defaultdict(float))
    marked_sold: dict = field(default_factory=lambda: defaultdict(float)); marked_bought: dict = field(default_factory=lambda: defaultdict(float))
    cap_net: dict = field(default_factory=lambda: defaultdict(float)); cap_uncounted: dict = field(default_factory=lambda: defaultdict(float))
    class_floors: dict = field(default_factory=dict); last_limit_reason: dict = field(default_factory=dict)
    snaps: dict = field(default_factory=dict); reserve_snaps: dict = field(default_factory=dict); prices: dict = field(default_factory=dict); brakes: int = 0; penalties: float = 0.; band_penalties: float = 0.; max_drawdown: float = 0.
    def __post_init__(self): self.initial_price = self.pool.price()
    def wallet(self, name, creator=False):
        w = self.wallets.setdefault(name, Wallet(name)); w.creator |= creator; return w
    def bucket(self, hour, marked=False): return int(hour // (24 if marked else self.policy.window))
    def cap_bucket(self, hour): return int(hour // self.policy.window)
    def price(self): return self.pool.price()
    def creator_tokens(self): return sum(w.tokens for w in self.wallets.values() if w.creator)
    def live_class(self,name,hour):
        w=self.wallet(name)
        if w.ever_large:return 'large'
        return 'band' if w.tokens>ALLOWANCE*self.reserve_snapshot(hour) else 'small'
    def class_floor(self,name,hour):
        key=(name,self.bucket(hour))
        live=self.live_class(name,hour)
        self.class_floors.setdefault(key,'band' if live!='small' else 'small')
        return key
    def account_class(self,name,hour):
        key=self.class_floor(name,hour)
        live=self.live_class(name,hour)
        return 'band' if live=='small' and self.class_floors[key]=='band' else live
    def raise_class_floor(self,name,hour):
        key=self.class_floor(name,hour);live=self.live_class(name,hour)
        self.class_floors[key]='band' if live!='small' or self.class_floors[key]=='band' else 'small'
    def snapshot(self, hour, block):
        if block <= self.policy.n: return
        k = self.bucket(hour, True)
        if k not in self.snaps:
            amounts = {n:w.marked for n,w in self.wallets.items() if w.marked}; self.snaps[k] = amounts, sum(amounts.values()), self.pool.token
    def reserve_snapshot(self, hour):
        return self.reserve_snaps.setdefault(self.bucket(hour), self.pool.token)
    def buy(self, name, near, hour, early=False, block=10_000, creator=False):
        self.snapshot(hour, block); self.reserve_snapshot(hour); self.class_floor(name,hour); marked = block <= self.policy.n or creator or self.wallet(name).creator
        b=max(0,block)
        extra=.49*max(0.,18-min(18,b))/18 if b < 18 else 0.
        estimate = self.pool.token - self.pool.token * self.pool.near / (self.pool.near + near * (1-FEE-extra))
        group = creator or self.wallet(name).creator
        if group and self.creator_tokens() + estimate > .04 * SUPPLY: return 0.
        out = self.pool.buy(near, self.policy.creator_fee_share, extra); w = self.wallet(name, group)
        w.tokens += out; w.marked += out if marked or group else 0.; w.ever_large |= w.tokens >= self.policy.threshold * SUPPLY
        if marked or group:self.marked_bought[name,self.bucket(hour,True)]+=out
        self.raise_class_floor(name,hour)
        if not marked and not group: self.unmarked_bought[name, self.bucket(hour)] += out
        self.record_cap_buy(name, hour, near)
        return out
    def transfer(self, source, target, amount, block, target_creator=False, hour=0.):
        self.reserve_snapshot(hour)
        a,b = self.wallet(source),self.wallet(target,target_creator)
        self.class_floor(source,hour);self.class_floor(target,hour)
        if a.ever_large or amount > a.tokens or (not b.ever_large and b.tokens+amount>=self.policy.threshold*SUPPLY) or (b.creator and self.creator_tokens() + amount > .04*SUPPLY): return False
        marked=min(amount,a.marked); a.tokens-=amount; a.marked-=marked; b.tokens+=amount; b.marked+=amount if b.creator else marked
        self.raise_class_floor(source,hour);self.raise_class_floor(target,hour);return True
    def cap_key(self, name, hour): return name, self.cap_bucket(hour)
    def record_cap_buy(self, name, hour, near):
        key=self.cap_key(name,hour);offset=min(near,self.cap_uncounted[key])
        self.cap_uncounted[key]-=offset;self.cap_net[key]-=near-offset
    def record_cap_sell(self, name, hour, counted=0., uncounted=0.):
        key=self.cap_key(name,hour);offset=min(uncounted,max(0.,-self.cap_net[key]))
        self.cap_net[key]+=offset;self.cap_uncounted[key]+=uncounted-offset;self.cap_net[key]+=counted
    def account_cap(self, name, hour, counted=0., uncounted=0.):
        key=self.cap_key(name,hour);net=self.cap_net[key];offset=min(uncounted,max(0.,-net))
        return max(0.,net+offset+counted)
    def cap_used(self, hour, adding_name=None, counted=0., uncounted=0.):
        bucket=self.cap_bucket(hour);names={name for name,k in self.cap_net if k==bucket} | {name for name,k in self.cap_uncounted if k==bucket}
        return sum(self.account_cap(name,hour,counted if name==adding_name else 0.,uncounted if name==adding_name else 0.) for name in names | ({adding_name} if adding_name else set()))
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
    def cap_fill_band(self,name,wanted,always_fill,penalty_free,uncounted,hour):
        def fits(total):
            gross=min(self.pool.gross(total),self.pool.real);uncounted_gross=min(self.pool.gross(min(total,uncounted)),self.pool.real)
            penalty_free_gross=min(self.pool.gross(min(total,penalty_free)),self.pool.real)
            penalty=max(0.,gross-penalty_free_gross)*(1-FEE)*self.policy.penalty
            counted=max(0.,gross*(1-FEE)-penalty-uncounted_gross*(1-FEE))
            creator=gross*FEE*self.policy.creator_fee_share
            payout=gross*(1-FEE)-penalty
            return self.cap_used(hour,name,counted,uncounted_gross*(1-FEE))<=CAP*(self.pool.real-payout-creator)
        if fits(wanted):return wanted
        lo,hi=min(wanted,always_fill),wanted
        if hi<=lo:return lo
        for _ in range(100):
            mid=(lo+hi)/2
            if fits(mid):lo=mid
            else:hi=mid
        return lo
    def floor_tokens(self, name, hour, marked):
        reserve=self.reserve_snapshot(hour); bucket=self.bucket(hour,marked);key=(name,bucket)
        if marked:
            amounts,total,reserve=self.snaps[self.bucket(hour,True)]; allowance=FLOOR*reserve*amounts.get(name,0.)/total if total else 0.
        else: allowance=FLOOR*reserve
        bought=self.marked_bought[key] if marked else self.unmarked_bought[key]
        sold=self.marked_sold[key] if marked else self.unmarked_sold[key]
        net_sold=max(0.,sold-bought)
        return max(0.,allowance-net_sold)
    def sell(self, name, wanted, hour, block=10_000):
        self.reserve_snapshot(hour);self.snapshot(hour, block);w=self.wallet(name);self.class_floor(name,hour);account_class=self.account_class(name,hour);wanted=min(wanted,w.tokens)
        marked=min(wanted,w.marked);unmarked_balance=max(0.,w.tokens-w.marked)
        if block<=self.policy.n: marked=0.
        total=payout=marked_sold=0.;limit_reason=None
        if marked:
            k=self.bucket(hour,True); amounts,supply,reserve=self.snaps[k]; allowance=ALLOWANCE*reserve*amounts.get(name,0.)/supply if supply else 0.
            wanted_marked=min(marked,max(0.,allowance-self.marked_sold[name,k])); cap_amount=self.cap_fill(name,wanted_marked,hour,0.);amount=cap_amount
            if cap_amount < wanted_marked:
                floor_amount=self.floor_tokens(name,hour,True)
                amount=min(marked,max(cap_amount,floor_amount))
                limit_reason='floor' if amount>cap_amount and amount>0 else 'cap_full'
            elif marked>wanted_marked:limit_reason='allowance'
            paid,fee,_=self.pool.sell(amount,self.policy.creator_fee_share); self.marked_sold[name,k]+=amount; self.record_cap_sell(name,hour,counted=paid); self.penalties+=fee; total+=amount;payout+=paid
            marked_sold+=amount
        unmarked=min(unmarked_balance,wanted if block<=self.policy.n else max(0.,wanted-marked_sold))
        if unmarked:
            k,ck=self.bucket(hour),self.cap_bucket(hour);allowance=ALLOWANCE*self.reserve_snapshot(hour)
            room=max(0.,allowance+self.unmarked_bought[name,k]-self.unmarked_sold[name,k])
            sellable=unmarked if account_class in {'small','band'} else min(unmarked,room)
            base=sellable if account_class=='small' else min(sellable,max(0.,allowance-self.unmarked_sold[name,k]))
            extra=max(0.,sellable-base) if account_class=='band' else 0.
            start=self.prices.setdefault(ck,self.price())
            if extra and self.policy.brake is not None and 1-self.price()/start>=self.policy.brake: self.brakes+=1;extra=0.;sellable=base
            if account_class=='large':
                wanted_base=sellable;cap_amount=self.cap_fill(name,wanted_base,hour,0.);base=cap_amount
                if cap_amount<wanted_base:
                    floor_amount=self.floor_tokens(name,hour,False);base=min(unmarked,max(cap_amount,floor_amount))
                    limit_reason='floor' if base>cap_amount and base>0 else 'cap_full'
                elif sellable<unmarked:limit_reason='allowance'
                if base:
                    paid,fee,_=self.pool.sell(base,self.policy.creator_fee_share);self.record_cap_sell(name,hour,counted=paid);self.unmarked_sold[name,k]+=base;self.penalties+=fee;total+=base;payout+=paid
            elif account_class=='band':
                total_sell=base+extra
                if total_sell:
                    always_fill=min(total_sell,room);penalty_free=min(total_sell,room);uncounted=base
                    allowed=self.cap_fill_band(name,total_sell,always_fill,penalty_free,uncounted,hour)
                    paid,fee,band,counted=self.pool.sell_band(allowed,uncounted,penalty_free,self.policy.creator_fee_share,self.policy.penalty)
                    self.unmarked_sold[name,k]+=allowed;self.record_cap_sell(name,hour,counted=counted,uncounted=max(0.,paid-counted))
                    self.penalties+=fee;self.band_penalties+=band;total+=allowed;payout+=paid
                    if allowed<total_sell:limit_reason='cap_full'
                if sellable<unmarked and limit_reason is None:limit_reason='allowance'
            elif base:
                paid,fee,_=self.pool.sell(base,self.policy.creator_fee_share);self.unmarked_sold[name,k]+=base;self.record_cap_sell(name,hour,uncounted=paid);self.penalties+=fee;total+=base;payout+=paid
        w.tokens-=total;w.marked-=marked_sold;w.received+=payout
        if w.tokens == 0:
            w.ever_large=False
        self.raise_class_floor(name,hour)
        if total>=wanted-1e-6:limit_reason=None
        if total<wanted-1e-6 and limit_reason is None:limit_reason='allowance'
        self.last_limit_reason[name]=limit_reason if total>1e-9 or limit_reason!='floor' else 'cap_full'
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
def cap_cases(output=None):
    """Export reproducible examples, including each launch-fee block."""
    cases=[]
    def start(e,hour=0):
        holders=[{'account':w.name,'tokens_base':unit(w.tokens,1e18),'marked_base':unit(w.marked,1e18),'large':w.ever_large,'class':e.live_class(w.name,hour)} for w in e.wallets.values()]
        reserve_base=int(unit(SUPPLY,1e18))-sum(int(w['tokens_base']) for w in holders)
        return {'real_near_yocto':unit(e.pool.real,1e24),'virtual_near_yocto':unit(e.pool.near-e.pool.real,1e24),
                'token_reserve_base':str(reserve_base),'holders':holders,'total_supply_base':unit(SUPPLY,1e18),
                'cap_used_by_account_yocto':{n:unit(e.account_cap(n,hour),1e24) for n,k in e.cap_net if k==e.cap_bucket(hour)},
                'rules':{'p_bps':200,'window_unmarked_s':int(e.policy.window*3600),'window_early_s':86400,'early_blocks':e.policy.n,
                         'large_threshold_bps':int(e.policy.threshold*10000),'creator_cap_bps':400,'real_cap_c_bps':int(CAP*10000),'floor_bps':int(FLOOR*10000),
                         'penalty_bps':int(e.policy.penalty*10000),'swap_fee_bps':int(FEE*10000),'early_fee_start_bps':5000,
                         'brake_drop_bps':None if e.policy.brake is None else int(e.policy.brake*10000),
                         'creator_fee_share_bps':int(e.policy.creator_fee_share*10000),'virtual_near':unit(VIRTUAL,1e24)}}
    def case(name,state,actions,**extra):cases.append({'name':name,'start_state':state,'actions':actions,**extra})
    def buy(e,actions,name,near,block,second=0,creator=False):
        h=second/3600;class_before=e.account_class(name,h);large_before=e.wallet(name).ever_large;out=e.buy(name,near,h,block=block,creator=creator)
        actions.append({'account':name,'kind':'buy','amount_near_yocto':unit(near,1e24),'time_s':second,'block':block,'result':{'tokens_filled_base':unit(out,1e18),'large_before':large_before,'large_after':e.wallet(name).ever_large,'class_before':class_before,'class_after':e.account_class(name,h),'cap_used_yocto':unit(e.cap_used(h),1e24)}})
    def sell(e,actions,name,amount,second=0,block=100):
        h=second/3600;e.reserve_snapshot(h);e.snapshot(h,block);w=e.wallet(name);class_before=e.account_class(name,h);all_balance=amount=='all'
        requested=w.tokens if all_balance else amount
        prior_penalty=e.band_penalties;filled,paid=e.sell(name,requested,h,block);class_after=e.account_class(name,h);reason=e.last_limit_reason.get(name)
        action={'account':name,'kind':'sell','amount':'all' if all_balance else unit(requested,1e18),'time_s':second,'block':block,
                'result':{'tokens_filled_base':unit(filled,1e18),'near_out_yocto':unit(paid,1e24),'fill':'full' if filled>=requested-1e-6 else ('none' if filled<=1e-9 else 'partial'),'limit_reason':reason,'penalty_near':unit(e.band_penalties-prior_penalty,1e24),'cap_used_yocto':unit(e.cap_used(h),1e24),'class_before':class_before,'class_after':class_after}}
        if not all_balance:action['amount_token_base']=unit(requested,1e18)
        actions.append(action)
    p=Policy(17,.02,None,.1)
    # The band holder starts at 2%; a buy crosses the 5% threshold.
    band_policy=Policy(17,.05,None,.1);e=Engine(band_policy,Pool(token=SUPPLY-20_000_000.,near=100.,real=0.));e.wallet('band').tokens=20_000_000.;state=start(e);actions=[];buy(e,actions,'band',4.,100);sell(e,actions,'band','all',1);case('threshold-cross-at-2-percent',state,actions,note='Cap is measured in real NEAR.')
    e=Engine(band_policy,Pool(token=SUPPLY-20_000_000.,near=105.,real=5.));e.wallet('band').tokens=20_000_000.;state=start(e);actions=[];buy(e,actions,'band',.1,100);sell(e,actions,'band',e.wallet('band').tokens,1);case('band-sell-above-allowance',state,actions)
    e=Engine(band_policy,Pool(token=SUPPLY-12_000_000.,near=110.,real=10.));e.wallet('band').tokens=12_000_000.;state=start(e);actions=[];sell(e,actions,'band',5_000_000.,0);sell(e,actions,'band',7_000_000.,1);case('band-class-floor-two-sells',state,actions)
    # Each launch-relative block has an explicit buy vector.
    for b in range(18):
        e=Engine(p,Pool(token=SUPPLY,near=100.,real=0.));state=start(e);actions=[];buy(e,actions,'early-buyer',1.,b);case(f'early-buy-block-{b}',state,actions)
    e=Engine(p,Pool(token=SUPPLY,near=100.,real=0.));state=start(e);actions=[];buy(e,actions,'marked',1.,0);buy(e,actions,'other-marked',9.,0);sell(e,actions,'marked',e.wallet('marked').tokens,1,18);case('marked-allowance-limit',state,actions)
    # The cap record and pool state precede the setup buy; that buy opens cap room and crosses T.
    full_policy=Policy(17,.05,None,.1);e=Engine(full_policy,Pool(token=SUPPLY-40_000_000.,near=110.,real=10.));e.wallet('large').tokens=40_000_000.;e.cap_net['other',1]=CAP*e.pool.real;state=start(e,1);actions=[];buy(e,actions,'large',3.,100,3600);sell(e,actions,'large','all',3601)
    if e.wallet('large').tokens>1e-9:sell(e,actions,'large','all',3602)
    case('full-cap-floor-and-second-sell',state,actions)
    for share in (0.,.3):
        policy=Policy(17,.02,None,.1,share);e=Engine(policy,Pool(token=SUPPLY,near=110.,real=10.));state=start(e);actions=[]
        buy(e,actions,'large',3.,100);sell(e,actions,'large',e.wallet('large').tokens,3600);case(f'post-sell-cap-s-{int(share*100)}',state,actions)
        e=Engine(policy,Pool(token=SUPPLY,near=110.,real=10.));state=start(e);actions=[]
        buy(e,actions,'round-trip',100.,100);sell(e,actions,'round-trip','all',1);case(f'same-account-round-trip-s-{int(share*100)}',state,actions)
    # The two action orders leave the same account cap contribution: buys first
    # offset uncounted small sells, and sells first leave credit for a later buy.
    for buy_first in (True,False):
        e=Engine(p,Pool(near=110.,real=10.));e.wallet('small').tokens=5_000_000.;state=start(e);actions=[]
        sequence=[('buy',.05),('sell','all')] if buy_first else [('sell','all'),('buy',.05)]
        for offset,(kind,amount) in enumerate(sequence):
            if kind=='buy':buy(e,actions,'small',amount,100,offset)
            else:sell(e,actions,'small',amount,offset)
        case(f'order-independence-{"buy-first" if buy_first else "sell-first"}',state,actions)
    e=Engine(p,Pool(near=110.,real=10.));e.wallet('band').tokens=11_000_000.;state=start(e);actions=[]
    for i in range(10):
        out=e.buy('band',.01,i/3600,block=100);actions.append({'account':'band','kind':'buy','amount_near_yocto':unit(.01,1e24),'time_s':i,'block':100,'result':{'tokens_filled_base':unit(out,1e18),'cap_used_yocto':unit(e.cap_used(i/3600),1e24)}})
        sell(e,actions,'band',out,i+0.5)
    buy(e,actions,'band',1.2,100,10);sell(e,actions,'band','all',10.5);case('band-round-trip-credit',state,actions)
    target=Path(output) if output else ROOT/'results'/'cap-cases.json'
    target.parent.mkdir(parents=True,exist_ok=True)
    target.write_text(json.dumps({'version':'1.0.0-rc.5','math':'binary floating point; 100 bisection iterations; serialized values rounded to nearest base unit','cases':cases},indent=2)+'\n')
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
