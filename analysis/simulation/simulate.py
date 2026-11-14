#!/usr/bin/env python3
"""Deterministic synthetic exercise of the Issue #2 sell rules."""
from __future__ import annotations
import argparse, csv, json, random, math
from collections import defaultdict
from dataclasses import dataclass, field
from decimal import Decimal, ROUND_DOWN, ROUND_HALF_UP, localcontext
from pathlib import Path

ROOT = Path(__file__).parent
SUPPLY, VIRTUAL, FEE, CAP, FLOOR = 1e9, 100., .01, .05, .001
ALLOWANCE = 1 / math.sqrt(1 - .02) - 1
TOKEN_BASE = 10**18
def token_floor(value): return int((Decimal(str(value))*Decimal('1e18')).quantize(Decimal('1'),rounding=ROUND_DOWN))/1e18
def token_base_floor(value): return int((Decimal(str(value))*Decimal('1e18')).quantize(Decimal('1'),rounding=ROUND_DOWN))
def token_amount(base_units): return Decimal(base_units)/Decimal(TOKEN_BASE)
def near_floor(value): return int((Decimal(str(value))*Decimal('1e24')).quantize(Decimal('1'),rounding=ROUND_DOWN))/1e24
def near_ceil(value): return int((Decimal(str(value))*Decimal('1e24')).quantize(Decimal('1'),rounding='ROUND_CEILING'))/1e24

@dataclass(frozen=True)
class Policy:
    n: int; threshold: float; brake: float | None; penalty: float; creator_fee_share: float = 0.; window: float = 1.

@dataclass
class Pool:
    token: float = SUPPLY; near: float = VIRTUAL; real: float = 0.; creator_fees: float = 0.
    last_buy_tokens_base: int = 0
    def price(self): return self.near / self.token
    def gross(self, tokens):
        if tokens<=0:return 0.
        with localcontext() as ctx:
            ctx.prec=80
            return near_floor(Decimal(str(self.near))*Decimal(str(tokens))/(Decimal(str(self.token))+Decimal(str(tokens))))
    def buy(self, near, share, extra=0.):
        creator = near * FEE * share; effective = near * (1 - FEE - extra)
        with localcontext() as ctx:
            ctx.prec=80
            token_reserve,near_reserve=Decimal(str(self.token)),Decimal(str(self.near))
            effective_near=Decimal(str(near))*(1-Decimal(str(FEE))-Decimal(str(extra)))
            output=token_reserve*effective_near/(near_reserve+effective_near)
            self.last_buy_tokens_base=int((output*Decimal('1e18')).to_integral_value(rounding=ROUND_DOWN))
        out = self.last_buy_tokens_base / 1e18
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
    tokens_base: int | None = None; marked_base: int | None = None

@dataclass
class Engine:
    policy: Policy; pool: Pool = field(default_factory=Pool); wallets: dict = field(default_factory=dict)
    unmarked_sold: dict = field(default_factory=lambda: defaultdict(float)); unmarked_bought: dict = field(default_factory=lambda: defaultdict(float))
    marked_sold: dict = field(default_factory=lambda: defaultdict(float)); marked_sold_hour: dict = field(default_factory=lambda: defaultdict(float)); marked_bought: dict = field(default_factory=lambda: defaultdict(float))
    cap_bought_tokens: dict = field(default_factory=lambda: defaultdict(float))
    cap_net: dict = field(default_factory=lambda: defaultdict(float))
    cap_marked_gross: dict = field(default_factory=lambda: defaultdict(float))
    class_floors: dict = field(default_factory=dict); last_limit_reason: dict = field(default_factory=dict); last_refusal_code: dict = field(default_factory=dict)
    last_counted_near: dict = field(default_factory=dict); last_sell_tokens_base: dict = field(default_factory=dict)
    snaps: dict = field(default_factory=dict); reserve_snaps: dict = field(default_factory=dict); prices: dict = field(default_factory=dict); brakes: int = 0; penalties: float = 0.; band_penalties: float = 0.; max_drawdown: float = 0.
    def __post_init__(self): self.initial_price = self.pool.price()
    def wallet(self, name, creator=False):
        w = self.wallets.setdefault(name, Wallet(name)); w.creator |= creator; return w
    def wallet_base(self,name,marked=False):
        w=self.wallet(name);attr='marked_base' if marked else 'tokens_base'
        if getattr(w,attr) is None:setattr(w,attr,token_base_floor(w.marked if marked else w.tokens))
        return getattr(w,attr)
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
        self.pool.last_buy_tokens_base=0
        self.snapshot(hour, block); self.reserve_snapshot(hour); self.class_floor(name,hour); marked = block <= self.policy.n or creator or self.wallet(name).creator
        b=max(0,block)
        extra=.49*max(0.,18-min(18,b))/18 if b < 18 else 0.
        estimate = self.pool.token - self.pool.token * self.pool.near / (self.pool.near + near * (1-FEE-extra))
        group = creator or self.wallet(name).creator
        if group and self.creator_tokens() + estimate > .04 * SUPPLY: return 0.
        out = self.pool.buy(near, self.policy.creator_fee_share, extra); w = self.wallet(name, group)
        w.tokens_base=self.wallet_base(name)+self.pool.last_buy_tokens_base
        w.marked_base=self.wallet_base(name,True)+(self.pool.last_buy_tokens_base if marked or group else 0)
        w.tokens=w.tokens_base/TOKEN_BASE;w.marked=w.marked_base/TOKEN_BASE
        w.ever_large |= w.tokens >= self.policy.threshold * SUPPLY
        if marked or group:self.marked_bought[name,self.bucket(hour,True)]+=out
        self.raise_class_floor(name,hour)
        if not marked and not group: self.unmarked_bought[name, self.bucket(hour)] += out
        if not marked and not group: self.cap_bought_tokens[name,self.cap_bucket(hour)]+=out
        self.record_cap_buy(name, hour, near)
        return out
    def transfer(self, source, target, amount, block, target_creator=False, hour=0.):
        self.reserve_snapshot(hour)
        a,b = self.wallet(source),self.wallet(target,target_creator)
        a_tokens=self.wallet_base(source);a_marked=self.wallet_base(source,True)
        b_tokens=self.wallet_base(target);b_marked=self.wallet_base(target,True)
        self.class_floor(source,hour);self.class_floor(target,hour)
        if a.ever_large or amount > a.tokens or (not b.ever_large and b.tokens+amount>=self.policy.threshold*SUPPLY) or (b.creator and self.creator_tokens() + amount > .04*SUPPLY): return False
        amount_base=token_base_floor(amount);marked_base=min(amount_base,a_marked)
        a.tokens_base=a_tokens-amount_base;a.marked_base=a_marked-marked_base
        b.tokens_base=b_tokens+amount_base;b.marked_base=b_marked+(amount_base if b.creator else marked_base)
        a.tokens=a.tokens_base/TOKEN_BASE;a.marked=a.marked_base/TOKEN_BASE
        b.tokens=b.tokens_base/TOKEN_BASE;b.marked=b.marked_base/TOKEN_BASE
        self.raise_class_floor(source,hour);self.raise_class_floor(target,hour);return True
    def cap_key(self, name, hour): return name, self.cap_bucket(hour)
    def record_cap_buy(self, name, hour, near):
        self.cap_net[self.cap_key(name,hour)]-=near
    def record_cap_sell(self, name, hour, counted=0.):
        self.cap_net[self.cap_key(name,hour)]+=counted
    def account_cap(self, name, hour, counted=0., marked=0.):
        key=self.cap_key(name,hour)
        return self.cap_marked_gross[key]+marked+max(0.,self.cap_net[key]+counted)
    def cap_used(self, hour, adding_name=None, counted=0., marked=0.):
        bucket=self.cap_bucket(hour)
        names={n for n,k in self.cap_net if k==bucket}|{n for n,k in self.cap_marked_gross if k==bucket}
        return sum(self.account_cap(n,hour,counted if n==adding_name else 0.,marked if n==adding_name else 0.) for n in names|({adding_name} if adding_name else set()))
    def cap_left(self,hour):
        c=CAP;f=FEE;s=self.policy.creator_fee_share
        with localcontext() as ctx:
            ctx.prec=80
            c,f,s=map(Decimal,map(str,(c,f,s)))
            value=(c*Decimal(str(self.pool.real))-Decimal(str(self.cap_used(hour))))*(1-f)/((1+c)*(1-f)+c*s*f)
            return float(max(Decimal(0),value))
    def record_marked_sell(self,name,hour,paid):
        self.cap_marked_gross[self.cap_key(name,hour)]+=paid
    def search_fill(self, allowed, most, fits):
        lo=max(0,allowed);hi=max(lo,most)
        if hi==lo or fits(hi):return hi
        while hi-lo>1:
            mid=(lo+hi)//2
            if fits(mid):lo=mid
            else:hi=mid
        return lo
    def floor_tokens(self, name, hour, marked):
        reserve=self.reserve_snapshot(hour);cap_bucket=self.cap_bucket(hour)
        if marked:
            bucket=self.bucket(hour,True);amounts,total,marked_reserve=self.snaps[bucket]
            allowance=FLOOR*marked_reserve*amounts.get(name,0.)/total if total else 0.
            used=self.marked_sold[name,bucket]+self.unmarked_sold[name,cap_bucket]
        else:
            allowance=FLOOR*reserve
            used=self.unmarked_sold[name,cap_bucket]+self.marked_sold_hour[name,cap_bucket]
        return max(0.,allowance-used)
    def quote_sell(self, name, marked, unmarked, bought_not_sold, never_counted,
                   penalty_free, hour):
        marked,unmarked=token_amount(marked),token_amount(unmarked)
        bought_not_sold=Decimal(str(bought_not_sold))
        never_counted=Decimal('Infinity') if never_counted==math.inf else token_amount(never_counted)
        penalty_free=Decimal('Infinity') if penalty_free==math.inf else token_amount(penalty_free)
        tokens=marked+unmarked
        gross=min(self.pool.gross(tokens),self.pool.real)
        fee=near_ceil(gross*FEE)
        free_tokens=marked+min(unmarked,penalty_free)
        free_gross=min(self.pool.gross(free_tokens),self.pool.real)
        excess=max(0.,gross-free_gross)
        excess_after_fee=max(0.,excess-near_floor(excess*FEE))
        penalty=min(near_ceil(excess_after_fee*self.policy.penalty),max(0.,gross-fee))
        payout=max(0.,gross-fee-penalty)
        creator=near_floor(fee*self.policy.creator_fee_share)
        bought=min(unmarked,bought_not_sold)
        held=unmarked-bought
        uncounted_tokens=min(held,never_counted)
        uncounted_start=marked+bought
        uncounted=max(0.,min(self.pool.gross(uncounted_start+uncounted_tokens),self.pool.real)
                         -min(self.pool.gross(uncounted_start),self.pool.real))
        uncounted=max(0.,uncounted-near_floor(uncounted*FEE))
        uncounted=min(payout,uncounted)
        marked_out=min(payout-uncounted,near_ceil(min(self.pool.gross(marked),self.pool.real)*(1-FEE)))
        counted=max(0.,payout-uncounted-marked_out)
        cap=self.cap_used(hour,name,counted,marked_out)
        return {'tokens':tokens,'gross':gross,'fee':fee,'creator':creator,'penalty':penalty,
                'payout':payout,'marked_out':marked_out,'counted':counted,'bought_sold':bought,
                'cap':cap}
    def sell(self, name, wanted, hour, block=10_000, wanted_base=None):
        self.reserve_snapshot(hour);self.snapshot(hour,block);w=self.wallet(name)
        self.class_floor(name,hour);account_class=self.account_class(name,hour)
        requested_base=(min(token_base_floor(wanted),self.wallet_base(name)) if wanted_base is None
                        else min(wanted_base,self.wallet_base(name)))
        requested=requested_base/1e18
        self.last_refusal_code[name]=None
        if block<=self.policy.n and w.marked>0:
            self.last_refusal_code[name]='FC_EARLY_WINDOW'
            self.last_limit_reason[name]=None;self.last_counted_near[name]=0.;self.last_sell_tokens_base[name]=0
            return 0.,0.
        cap_bucket=self.cap_bucket(hour);marked_bucket=self.bucket(hour,True)
        marked_asked=min(requested,w.marked)
        marked_amounts,marked_supply,marked_reserve=self.snaps.get(
            marked_bucket,({},0.,self.pool.token))
        marked_allowance=ALLOWANCE*marked_reserve*marked_amounts.get(name,0.)/marked_supply if marked_supply else 0.
        marked_most_base=min(requested_base,self.wallet_base(name,True),token_base_floor(max(0.,marked_allowance-self.marked_sold[name,marked_bucket])))
        unmarked_balance_base=max(0,self.wallet_base(name)-self.wallet_base(name,True))
        unmarked_asked_base=min(max(0,requested_base-marked_most_base),unmarked_balance_base)

        allowance=ALLOWANCE*self.reserve_snapshot(hour);window_bucket=self.bucket(hour)
        bought_key=(name,cap_bucket);bought_not_sold=self.cap_bought_tokens[bought_key]
        room=max(0.,allowance+self.unmarked_bought[name,window_bucket]-self.unmarked_sold[name,window_bucket])
        room_base=token_base_floor(room)
        floor_left_base=token_base_floor(self.floor_tokens(name,hour,False))
        if account_class=='large':
            unmarked_most_base=min(unmarked_asked_base,room_base)
            unmarked_always_base=min(unmarked_most_base,floor_left_base)
            penalty_free=math.inf;never_counted=0
            floor_before_base=floor_left_base
        elif account_class=='band':
            unmarked_always_base=min(unmarked_asked_base,room_base);unmarked_most_base=unmarked_asked_base
            start=self.prices.setdefault(cap_bucket,self.price())
            if unmarked_most_base>unmarked_always_base and self.policy.brake is not None and 1-self.price()/start>=self.policy.brake:
                self.brakes+=1;unmarked_most_base=unmarked_always_base
            penalty_free=room_base;never_counted=token_base_floor(max(0.,allowance-self.unmarked_sold[name,window_bucket]))
            floor_before_base=None
        else:
            unmarked_most_base=unmarked_asked_base
            unmarked_always_base=0 if bought_not_sold>0 else unmarked_asked_base
            penalty_free=math.inf;never_counted=math.inf;floor_before_base=None

        marked_floor_base=token_base_floor(self.floor_tokens(name,hour,True)) if marked_most_base else 0
        marked_always_base=min(marked_most_base,marked_floor_base)
        def unmarked_after_marked(marked_base):
            if floor_before_base is not None:return min(unmarked_most_base,max(0,floor_before_base-marked_base))
            return unmarked_always_base
        def fits(marked,unmarked):
            out=self.quote_sell(name,marked,unmarked,bought_not_sold,never_counted,penalty_free,hour)
            with localcontext() as ctx:
                ctx.prec=80
                real_after=Decimal(str(self.pool.real))-Decimal(str(out['payout']))-Decimal(str(out['creator']))
                return Decimal(str(out['cap']))<=Decimal(str(CAP))*real_after
        marked_filled=self.search_fill(
            marked_always_base,marked_most_base,
            lambda m:fits(m,unmarked_after_marked(m)))
        always_after_marked_base=unmarked_after_marked(marked_filled)
        used_by_marked=self.quote_sell(name,marked_filled,0,bought_not_sold,
                                       never_counted,penalty_free,hour)['cap']
        unmarked_filled=self.search_fill(
            always_after_marked_base,unmarked_most_base,
            lambda u:fits(marked_filled,u) or
                (account_class=='small' and self.quote_sell(
                    name,marked_filled,u,bought_not_sold,never_counted,
                    penalty_free,hour)['cap']<=used_by_marked))
        result=self.quote_sell(name,marked_filled,unmarked_filled,bought_not_sold,
                               never_counted,penalty_free,hour)
        marked_filled_base,unmarked_filled_base=marked_filled,unmarked_filled
        total_base=marked_filled_base+unmarked_filled_base
        marked_filled,unmarked_filled=marked_filled_base/TOKEN_BASE,unmarked_filled_base/TOKEN_BASE
        total=total_base/TOKEN_BASE;payout=result['payout']
        self.last_sell_tokens_base[name]=total_base
        if total:
            self.pool.token+=total;self.pool.near-=payout+result['creator']
            self.pool.real-=payout+result['creator'];self.pool.creator_fees+=result['creator']
            w.tokens_base-=total_base;w.marked_base-=marked_filled_base
            w.tokens=w.tokens_base/TOKEN_BASE;w.marked=w.marked_base/TOKEN_BASE;w.received+=payout
            self.marked_sold[name,marked_bucket]+=marked_filled
            self.marked_sold_hour[name,cap_bucket]+=marked_filled
            self.unmarked_sold[name,window_bucket]+=unmarked_filled
            self.cap_bought_tokens[bought_key]=max(0.,bought_not_sold-float(result['bought_sold']))
            if marked_filled:self.record_marked_sell(name,hour,result['marked_out'])
            if result['counted']:self.record_cap_sell(name,hour,result['counted'])
            self.penalties+=result['fee']-result['creator']+result['penalty']
            if account_class=='band':self.band_penalties+=result['penalty']
            if w.tokens==0:w.ever_large=False
        self.raise_class_floor(name,hour)
        unmarked_floor_left_base=max(0,floor_before_base-marked_filled_base) if floor_before_base is not None else 0
        unmarked_floor_used=(account_class=='large' and floor_before_base is not None and
                             unmarked_filled_base==unmarked_floor_left_base and unmarked_filled_base>0)
        if total_base>=requested_base:limit_reason=None
        elif marked_filled_base<marked_most_base:
            limit_reason='floor' if marked_filled_base==marked_always_base and (marked_filled_base>0 or unmarked_floor_used) else 'cap_full'
        elif unmarked_filled_base==unmarked_most_base:limit_reason='allowance'
        elif unmarked_floor_used:limit_reason='floor'
        else:limit_reason='cap_full'
        self.last_limit_reason[name]=limit_reason
        self.last_counted_near[name]=result['marked_out']+result['counted']
        return total,payout
    def split(self, source,count):
        w=self.wallet(source)
        if w.ever_large:return []
        names=[f'{source}-{i}' for i in range(count)];units=self.wallet_base(source);each=units//count
        for i,n in enumerate(names):
            target=self.wallet(n);part=each+(1 if i<units%count else 0)
            target.tokens_base=self.wallet_base(n)+part;target.marked_base=self.wallet_base(n,True)
            target.tokens=target.tokens_base/TOKEN_BASE;target.marked=target.marked_base/TOKEN_BASE
        w.tokens_base=0;w.marked_base=0;w.tokens=0.;w.marked=0.;return names

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
def scale_marked_supply(e,share):
    wallets=[w for w in e.wallets.values() if e.wallet_base(w.name,True)>0]
    total=sum(e.wallet_base(w.name,True) for w in wallets)
    target=token_base_floor(Decimal(str(share))*Decimal(str(SUPPLY)))
    scaled=[]
    for w in wallets:
        base=e.wallet_base(w.name,True);whole,remainder=divmod(base*target,total)
        scaled.append([w,whole,remainder])
    leftover=target-sum(item[1] for item in scaled)
    for item in sorted(scaled,key=lambda item:(-item[2],item[0].name))[:leftover]:item[1]+=1
    for w,base,_ in scaled:
        w.marked_base=w.tokens_base=base
        w.marked=w.tokens=base/TOKEN_BASE
def early_exit(p,share):
    e=seed(p);scale_marked_supply(e,share)
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
def unit_floor(value,scale): return str(int((Decimal(str(value))*Decimal(str(scale))).quantize(Decimal('1'),rounding=ROUND_DOWN)))
def cap_cases(output=None):
    """Export reproducible examples, including each launch-fee block."""
    cases=[]
    def start(e,hour=0):
        for w in e.wallets.values():
            w.tokens=token_floor(w.tokens);w.marked=min(token_floor(w.marked),w.tokens)
            w.tokens_base=token_base_floor(w.tokens);w.marked_base=token_base_floor(w.marked)
        holder_units={n:int(unit(w.tokens,1e18)) for n,w in e.wallets.items()}
        reserve_base=int(unit(SUPPLY,1e18))-sum(holder_units.values())
        e.pool.token=reserve_base/1e18
        holders=[{'account':w.name,'tokens_base':str(holder_units[n]),'marked_base':unit(w.marked,1e18),'large':w.ever_large,'class':e.live_class(w.name,hour),
                  'marked_sold_1h_base':unit(e.marked_sold_hour[n,e.cap_bucket(hour)],1e18),'unmarked_sold_1h_base':unit(e.unmarked_sold[n,e.cap_bucket(hour)],1e18),
                  'marked_sold_24h_base':unit(e.marked_sold[n,e.bucket(hour,True)],1e18),'bought_tokens_1h_base':unit(e.cap_bought_tokens[n,e.cap_bucket(hour)],1e18)} for n,w in e.wallets.items()]
        assert reserve_base+sum(holder_units.values())==int(unit(SUPPLY,1e18))
        early_snapshot=e.snaps.get(e.bucket(hour,True))
        return {'real_near_yocto':unit(e.pool.real,1e24),'virtual_near_yocto':unit(e.pool.near-e.pool.real,1e24),
                'token_reserve_base':str(reserve_base),'holders':holders,'total_supply_base':unit(SUPPLY,1e18),
                'creator_fee_share_bps':int(e.policy.creator_fee_share*10000),
                'cap_used_by_account_yocto':{n:unit(e.account_cap(n,hour),1e24) for n,k in sorted(set(e.cap_net)|set(e.cap_marked_gross)) if k==e.cap_bucket(hour)},
                'cap_net_signed_by_account_yocto':{n:unit(v,1e24) for (n,k),v in sorted(e.cap_net.items()) if k==e.cap_bucket(hour)},
                'marked_cap_gross_by_account_yocto':{n:unit(v,1e24) for (n,k),v in sorted(e.cap_marked_gross.items()) if k==e.cap_bucket(hour)},
                'cap_bought_tokens_base_by_account':{n:unit(v,1e18) for (n,k),v in sorted(e.cap_bought_tokens.items()) if k==e.cap_bucket(hour)},
                'cap_left_yocto':unit_floor(e.cap_left(hour),1e24),
                'reserve_snapshot_token_base':unit(e.reserve_snaps.get(e.cap_bucket(hour),e.pool.token),1e18),
                'early_snapshot':None if early_snapshot is None else {'marked_balances_base':{n:unit(v,1e18) for n,v in sorted(early_snapshot[0].items())},'marked_supply_base':unit(early_snapshot[1],1e18),'token_reserve_base':unit(early_snapshot[2],1e18)},
                'rules':{'p_bps':200,'window_unmarked_s':int(e.policy.window*3600),'window_early_s':86400,'early_blocks':e.policy.n,
                         'large_threshold_bps':int(e.policy.threshold*10000),'creator_cap_bps':400,'real_cap_c_bps':int(CAP*10000),'floor_bps':int(FLOOR*10000),
                         'penalty_bps':int(e.policy.penalty*10000),'swap_fee_bps':int(FEE*10000),'early_fee_start_bps':5000,
                         'brake_drop_bps':None if e.policy.brake is None else int(e.policy.brake*10000),
                         'creator_fee_share_bps':int(e.policy.creator_fee_share*10000),'virtual_near':unit(VIRTUAL,1e24)}}
    def case(name,state,actions,**extra):cases.append({'name':name,'start_state':state,'actions':actions,**extra})
    def buy(e,actions,name,near,block,second=0,creator=False):
        h=second/3600;class_before=e.account_class(name,h);large_before=e.wallet(name).ever_large;cap_left_before=e.cap_left(h);out=e.buy(name,near,h,block=block,creator=creator)
        actions.append({'account':name,'kind':'buy','amount_near_yocto':unit(near,1e24),'time_s':second,'block':block,'result':{'tokens_filled_base':str(e.pool.last_buy_tokens_base),'large_before':large_before,'large_after':e.wallet(name).ever_large,'class_before':class_before,'class_after':e.account_class(name,h),'cap_used_yocto':unit(e.cap_used(h),1e24),'cap_left_before_yocto':unit_floor(cap_left_before,1e24),'cap_left_yocto':unit_floor(e.cap_left(h),1e24)}})
        return out
    def sell(e,actions,name,amount,second=0,block=100,amount_base=None):
        h=second/3600;e.reserve_snapshot(h);e.snapshot(h,block);w=e.wallet(name);class_before=e.account_class(name,h);all_balance=amount=='all'
        requested=w.tokens if all_balance else amount
        requested_base=(e.wallet_base(name) if all_balance else
                        token_base_floor(requested) if amount_base is None else amount_base)
        prior_penalty=e.band_penalties;cap_left_before=e.cap_left(h);filled,paid=e.sell(name,requested,h,block,wanted_base=requested_base);class_after=e.account_class(name,h);reason=e.last_limit_reason.get(name)
        filled_base=e.last_sell_tokens_base.get(name,0)
        action={'account':name,'kind':'sell','amount':'all' if all_balance else str(requested_base),'time_s':second,'block':block,
                'result':{'tokens_filled_base':str(filled_base),'near_out_yocto':unit(paid,1e24),'counted_near_yocto':unit(e.last_counted_near.get(name,0.),1e24),'fill':'full' if filled_base>=requested_base else ('none' if filled_base==0 else 'partial'),'limit_reason':reason,'penalty_near':unit(e.band_penalties-prior_penalty,1e24),'cap_used_yocto':unit(e.cap_used(h),1e24),'cap_left_before_yocto':unit_floor(cap_left_before,1e24),'cap_left_yocto':unit_floor(e.cap_left(h),1e24),'class_before':class_before,'class_after':class_after}}
        if e.last_refusal_code.get(name):action['result']['refusal_code']=e.last_refusal_code[name]
        actions.append(action)
    p=Policy(17,.02,None,.1)
    # The band holder starts at 2%; a buy crosses the 5% threshold.
    band_policy=Policy(17,.05,None,.1);e=Engine(band_policy,Pool(token=SUPPLY-20_000_000.,near=100.,real=0.));e.wallet('band').tokens=20_000_000.;state=start(e);actions=[];buy(e,actions,'band',4.,100);sell(e,actions,'band','all',1);case('threshold-cross-at-2-percent',state,actions,note='Cap is measured in real NEAR.')
    e=Engine(band_policy,Pool(token=SUPPLY-20_000_000.,near=105.,real=5.));e.wallet('band').tokens=20_000_000.;state=start(e);actions=[];buy(e,actions,'band',.1,100);sell(e,actions,'band','all',1);case('band-sell-above-allowance',state,actions)
    e=Engine(band_policy,Pool(token=SUPPLY-12_000_000.,near=110.,real=10.));e.wallet('band').tokens=12_000_000.;state=start(e);actions=[];sell(e,actions,'band',5_000_000.,0);sell(e,actions,'band',7_000_000.,1);case('band-class-floor-two-sells',state,actions)
    # Each launch-relative block has an explicit buy vector.
    for b in range(18):
        e=Engine(p,Pool(token=SUPPLY,near=100.,real=0.));state=start(e);actions=[];buy(e,actions,'early-buyer',1.,b);case(f'early-buy-block-{b}',state,actions)
    e=Engine(p,Pool(token=SUPPLY,near=100.,real=0.));state=start(e);actions=[];buy(e,actions,'marked',1.,0);buy(e,actions,'other-marked',9.,0);sell(e,actions,'marked','all',1,18);case('marked-allowance-limit',state,actions)
    e=Engine(p,Pool(token=SUPPLY-2_000_000.,near=100.,real=0.));e.wallet('mixed').tokens=2_000_000.;e.wallet('mixed').marked=1_000_000.;state=start(e);actions=[];sell(e,actions,'mixed','all',1,17);case('early-window-refuses-mixed-sell',state,actions)
    # The cap record and pool state precede the setup buy; that buy opens cap room and crosses T.
    full_policy=Policy(17,.05,None,.1);e=Engine(full_policy,Pool(token=SUPPLY-40_000_000.,near=110.,real=10.));e.wallet('large').tokens=40_000_000.;e.cap_net['other',1]=CAP*e.pool.real;state=start(e,1);actions=[];buy(e,actions,'large',3.,100,3600);sell(e,actions,'large','all',3601)
    if e.wallet('large').tokens>1e-9:sell(e,actions,'large','all',3602)
    case('full-cap-floor-and-second-sell',state,actions)
    for share in (0.,.3):
        policy=Policy(17,.02,None,.1,share);e=Engine(policy,Pool(token=SUPPLY,near=110.,real=10.));state=start(e);actions=[]
        buy(e,actions,'large',3.,100);sell(e,actions,'large','all',3600);case(f'post-sell-cap-s-{int(share*100)}',state,actions)
        e=Engine(policy,Pool(token=SUPPLY,near=110.,real=10.));state=start(e);actions=[]
        buy(e,actions,'round-trip',100.,100);sell(e,actions,'round-trip','all',1);case(f'same-account-round-trip-s-{int(share*100)}',state,actions)
    # The two action orders leave the same account cap contribution: buys first
    # offset uncounted small sells, and sells first leave credit for a later buy.
    for buy_first in (True,False):
        e=Engine(p,Pool(token=SUPPLY-10_000_000.,near=200.,real=100.));w=e.wallet('mixed');w.tokens=10_000_000.;w.marked=5_000_000.
        state=start(e);actions=[]
        if buy_first:buy(e,actions,'mixed',.5,100,0)
        sell(e,actions,'mixed',5_000_000.,1 if buy_first else 0)
        sell(e,actions,'mixed',5_000_000.,2 if buy_first else 1)
        if not buy_first:buy(e,actions,'mixed',.5,100,2)
        case(f'order-independence-{"buy-first" if buy_first else "sell-first"}',state,actions)
    e=Engine(p,Pool(token=SUPPLY-11_000_000.,near=110.,real=10.));e.wallet('band').tokens=11_000_000.;state=start(e);actions=[]
    for i in range(10):
        second=2*i;buy(e,actions,'band',.01,100,second);bought_base=e.pool.last_buy_tokens_base;sell(e,actions,'band',bought_base/1e18,second+1,amount_base=bought_base)
    buy(e,actions,'band',1.2,100,20);sell(e,actions,'band','all',21);case('band-round-trip-credit',state,actions)
    e=Engine(band_policy,Pool(token=SUPPLY-40_000_000.,near=110.,real=10.));holder=e.wallet('holder');holder.tokens=40_000_000;holder.marked=20_000_000;holder.ever_large=True
    e.wallet('cap-other');e.cap_net['cap-other',0]=CAP*e.pool.real
    state=start(e);actions=[];marked_left=ALLOWANCE*e.pool.token
    sell(e,actions,'holder',marked_left+5_000_000,1,18);sell(e,actions,'holder',marked_left+5_000_000,2,18)
    case('floor-marked-first',state,actions)
    # The previous hour used the full marked allowance; this hour starts with a fresh unmarked floor.
    e=Engine(band_policy,Pool(token=SUPPLY-40_000_000.,near=110.,real=10.));holder=e.wallet('holder');holder.tokens=40_000_000;holder.marked=20_000_000;holder.ever_large=True
    early_total=20_000_000.;early_reserve=e.pool.token;marked_left=token_floor(ALLOWANCE*early_reserve)
    e.snaps[0]=({'holder':20_000_000.},early_total,early_reserve);e.reserve_snaps[0]=early_reserve
    prior_payout=e.pool.gross(marked_left)*(1-FEE)
    holder.tokens=token_floor(holder.tokens-marked_left);holder.marked=token_floor(holder.marked-marked_left);e.pool.token=token_floor(e.pool.token+marked_left);e.pool.near-=prior_payout;e.pool.real-=prior_payout
    e.marked_sold['holder',0]=marked_left;e.marked_sold_hour['holder',0]=marked_left
    e.cap_marked_gross['holder',0]=prior_payout
    e.wallet('cap-other');e.cap_net['cap-other',1]=CAP*e.pool.real
    state=start(e,1);actions=[]
    sell(e,actions,'holder',5_000_000,3601,18);sell(e,actions,'holder',marked_left+5_000_000,3602,18)
    case('floor-unmarked-first',state,actions)
    # Marked floor is exhausted, but the account still has unused unmarked floor.
    e=Engine(band_policy,Pool(token=SUPPLY-40_000_000.,near=110.,real=10.));holder=e.wallet('holder');holder.tokens=30_000_000;holder.marked=2_000_000;holder.ever_large=True
    early=e.wallet('early-peer');early.tokens=10_000_000;early.marked=18_000_000
    e.reserve_snaps[0]=e.pool.token;e.snaps[0]=({'holder':2_000_000,'early-peer':18_000_000},20_000_000,e.pool.token)
    prior_marked_floor=FLOOR*e.pool.token*.1;prior_payout=e.pool.gross(prior_marked_floor)*(1-FEE)
    holder.tokens-=prior_marked_floor;holder.marked-=prior_marked_floor;e.pool.token+=prior_marked_floor;e.pool.near-=prior_payout;e.pool.real-=prior_payout
    e.marked_sold['holder',0]=prior_marked_floor;e.marked_sold_hour['holder',0]=prior_marked_floor
    e.cap_marked_gross['holder',0]=prior_payout;e.cap_net['cap-other',0]=CAP*e.pool.real-prior_payout;e.wallet('cap-other')
    state=start(e);actions=[];marked_left=ALLOWANCE*e.snaps[0][2]*.1-e.marked_sold['holder',0]
    sell(e,actions,'holder',500_000,1);sell(e,actions,'holder',marked_left+1_000_000,2)
    case('marked-cap-hold-does-not-use-unmarked-floor',state,actions)
    # Report the exact formula before and after successive s=30% sells.
    e=Engine(Policy(17,.02,None,.1,.3),Pool(token=SUPPLY-100_000_000.,near=110.,real=10.));e.wallet('large').tokens=100_000_000;e.wallet('large').ever_large=True
    e.cap_net['other',0]=.1;e.wallet('other');state=start(e);actions=[]
    for second in (1,2,3):sell(e,actions,'large',5_000_000,second)
    case('cap-left-s-30',state,actions)
    # A band's first allowance is uncounted; its above-allowance part is cap-limited.
    e=Engine(band_policy,Pool(token=SUPPLY-30_000_000.,near=110.,real=10.));e.wallet('band').tokens=30_000_000
    e.cap_net['other',0]=.3;e.wallet('other');state=start(e);actions=[]
    sell(e,actions,'band',25_000_000,1);case('band-above-allowance-cap-left',state,actions)
    # Marked payout is gross; a later unmarked resale is netted separately.
    e=Engine(p,Pool(token=SUPPLY-1_000_000.,near=110.,real=10.));e.wallet('marked').tokens=1_000_000;e.wallet('marked').marked=1_000_000
    state=start(e);actions=[];sell(e,actions,'marked','all',1);buy(e,actions,'marked',.1,100,2);sell(e,actions,'marked','all',3)
    case('marked-sell-buy-resell',state,actions)
    # The same account's resale and another marked holder compete for one cap.
    e=Engine(p,Pool(token=SUPPLY-20_000_000.,near=200.,real=100.));e.wallet('a').tokens=10_000_000;e.wallet('a').marked=10_000_000
    e.wallet('b').tokens=10_000_000;e.wallet('b').marked=10_000_000;state=start(e);actions=[]
    sell(e,actions,'a','all',1);buy(e,actions,'a',.1,100,2);bought_base=e.pool.last_buy_tokens_base;sell(e,actions,'a',bought_base/1e18,3,amount_base=bought_base);sell(e,actions,'b','all',4)
    case('marked-resale-two-account-cap',state,actions)
    # Buying while the cap is full creates only the room attributable to that buy.
    e=Engine(p,Pool(near=110.,real=10.));e.wallet('cap-other');e.wallet('small-buyer');e.cap_net['cap-other',0]=CAP*e.pool.real
    state=start(e);actions=[];bought=buy(e,actions,'small-buyer',.1,100,1);sell(e,actions,'small-buyer','all',2)
    case('small-buy-sell-at-full-cap',state,actions)
    # Another buy raises price; only room above this wallet's buy is cap-limited.
    e=Engine(p,Pool(token=SUPPLY-200_000_000.,near=110.,real=10.));e.wallet('small-buyer')
    e.wallet('cap-other');e.wallet('price-buyer');e.wallet('large-seller').tokens=200_000_000;e.wallet('large-seller').ever_large=True
    e.cap_net['cap-other',0]=CAP*e.pool.real;state=start(e);actions=[]
    buy(e,actions,'small-buyer',.1,100,1);buy(e,actions,'price-buyer',10.,100,2)
    sell(e,actions,'large-seller',2_900_000,3);sell(e,actions,'small-buyer','all',4)
    case('small-buy-price-rise-cap',state,actions)
    # A holder with old unmarked tokens resells a same-window buy after its marked allowance.
    e=Engine(p,Pool(token=SUPPLY-30_000_000.,near=110.,real=10.));e.wallet('mixed').tokens=30_000_000;e.wallet('mixed').marked=10_000_000;e.wallet('mixed').ever_large=True
    state=start(e);actions=[];sell(e,actions,'mixed','all',1);buy(e,actions,'mixed',.1,100,2);bought_base=e.pool.last_buy_tokens_base;sell(e,actions,'mixed',bought_base/1e18,3,amount_base=bought_base)
    case('marked-and-unmarked-old-plus-buy-resell',state,actions)
    # Marked sales reduce the later unmarked floor within the same 1-hour window.
    e=Engine(band_policy,Pool(token=SUPPLY-40_000_000.,near=110.,real=10.));holder=e.wallet('holder');holder.tokens=30_000_000;holder.marked=10_000_000;holder.ever_large=True
    peer=e.wallet('early-peer');peer.tokens=10_000_000;peer.marked=10_000_000;e.wallet('cap-other')
    e.cap_net['cap-other',0]=CAP*e.pool.real;state=start(e);actions=[]
    sell(e,actions,'holder','all',1);sell(e,actions,'holder','all',2)
    case('marked-sale-reduces-unmarked-floor',state,actions)
    target=Path(output) if output else ROOT/'results'/'cap-cases.json'
    target.parent.mkdir(parents=True,exist_ok=True)
    target.write_text(json.dumps({'interface':'1.0.0-rc.7','version':'1.0.0-rc.7','math':'binary-float reserve state; integer token-base-unit fill search; gross output, fees and penalties quantized to yoctoNEAR; cap_left rounded down; other serialized amounts rounded to nearest base unit','cases':cases},indent=2)+'\n')
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
        return
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
