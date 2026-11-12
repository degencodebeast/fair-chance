#!/usr/bin/env python3
"""Deterministic synthetic exercise of the Issue #2 sell rules."""
from __future__ import annotations
import argparse, csv, random
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).parent
SUPPLY, VIRTUAL, FEE, ALLOWANCE, CAP, FLOOR = 1e9, 100., .01, .010153, .05, .001

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
        return payout, normal - creator + gross * penalty
    def tokens_for(self, payout, share, penalty=0.):
        lo, hi = 0., self.token
        for _ in range(64):
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
    cap_out: dict = field(default_factory=lambda: defaultdict(float)); cap_buy: dict = field(default_factory=lambda: defaultdict(float)); marked_out: dict = field(default_factory=lambda: defaultdict(float))
    snaps: dict = field(default_factory=dict); prices: dict = field(default_factory=dict); brakes: int = 0; penalties: float = 0.; max_drawdown: float = 0.
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
            amounts = {n:w.marked for n,w in self.wallets.items() if w.marked}; self.snaps[k] = amounts, sum(amounts.values())
    def buy(self, name, near, hour, early=False, block=10_000, creator=False):
        self.snapshot(hour, block); extra = .09 * max(0., 1 - block / self.policy.n) if early else 0.
        estimate = self.pool.token - self.pool.token * self.pool.near / (self.pool.near + near * (1-FEE-extra))
        group = creator or self.wallet(name).creator
        if group and self.creator_tokens() + estimate > .04 * SUPPLY: return 0.
        out = self.pool.buy(near, self.policy.creator_fee_share, extra); w = self.wallet(name, group)
        w.tokens += out; w.marked += out if early or group else 0.; w.ever_large |= w.tokens >= self.policy.threshold * SUPPLY
        self.bought[name, self.bucket(hour)] += out
        if not group: self.cap_buy[self.cap_bucket(hour)] += near
        return out
    def transfer(self, source, target, amount, block, target_creator=False):
        a,b = self.wallet(source),self.wallet(target,target_creator)
        if amount > a.tokens or (b.creator and self.creator_tokens() + amount > .04*SUPPLY): return False
        marked=min(amount,a.marked); a.tokens-=amount; a.marked-=marked; b.tokens+=amount; b.marked+=amount if b.creator else marked
        b.ever_large |= b.tokens >= self.policy.threshold*SUPPLY; return True
    def cap_room(self, hour):
        k=self.cap_bucket(hour); used=self.marked_out[k]+max(0.,self.cap_out[k]-self.cap_buy[k]); return max(0.,CAP*self.pool.real-used)
    def floor_tokens(self, penalty):
        lo,hi=0.,self.pool.token; floor=self.initial_price*FLOOR
        for _ in range(64):
            mid=(lo+hi)/2; gross=min(self.pool.gross(mid),self.pool.real)
            near=self.pool.near-gross*(1-FEE*(1-self.policy.creator_fee_share)-penalty)
            if near/(self.pool.token+mid)>=floor: lo=mid
            else: hi=mid
        return lo
    def limited(self, wanted, hour, penalty, counted):
        if not counted: return wanted
        room=self.cap_room(hour)
        return min(wanted, self.floor_tokens(penalty) if room <= 0 else self.pool.tokens_for(room,self.policy.creator_fee_share,penalty))
    def sell(self, name, wanted, hour, block=10_000):
        self.snapshot(hour, block); w=self.wallet(name); wanted=min(wanted,w.tokens); marked=min(wanted,w.marked); unmarked=wanted-marked
        if block<=self.policy.n: marked=0.
        total=payout=0.
        if marked:
            k=self.bucket(hour,True); amounts,supply=self.snaps[k]; allowance=ALLOWANCE*amounts.get(name,0.)
            amount=self.limited(min(marked,max(0.,allowance-self.sold[name,k])),hour,0.,True)
            paid,fee=self.pool.sell(amount,self.policy.creator_fee_share); self.sold[name,k]+=amount; self.marked_out[self.cap_bucket(hour)]+=paid; self.penalties+=fee; total+=amount;payout+=paid
        if unmarked:
            k,ck=self.bucket(hour),self.cap_bucket(hour); room=max(0.,ALLOWANCE*self.pool.token+self.bought[name,k]-self.sold[name,k])
            base=min(unmarked,room); extra=0. if w.ever_large else max(0.,unmarked-base)
            base=self.limited(base,hour,0.,w.ever_large)
            start=self.prices.setdefault(ck,self.price())
            if extra and self.policy.brake is not None and 1-self.price()/start>=self.policy.brake: self.brakes+=1;extra=0.
            extra=self.limited(extra,hour,self.policy.penalty,True)
            for amount,penalty,counted in ((base,0.,w.ever_large),(extra,self.policy.penalty,True)):
                if amount:
                    paid,fee=self.pool.sell(amount,self.policy.creator_fee_share,penalty); self.sold[name,k]+=amount
                    if counted:self.cap_out[ck]+=paid
                    self.penalties+=fee;total+=amount;payout+=paid
        w.tokens-=total;w.marked-=min(w.marked,total);w.received+=payout;return total,payout
    def split(self, source,count):
        w=self.wallet(source)
        if w.ever_large:return []
        names=[f'{source}-{i}' for i in range(count)];each=w.tokens/count
        for n in names:self.wallet(n).tokens+=each
        w.tokens=0.;return names

def seed(policy, early_near=3.):
    e=Engine(policy);e.buy('creator',4,0,True,0,True)
    for i in range(5):e.buy(f'early-{i}',early_near,0,True,i*3)
    e.buy('small-early',.05,0,True,3)
    for i in range(3):e.buy(f'large-{i}',35,48+i)
    for i in range(8):e.buy(f'band-{i}',4,72+i)
    return e

def normal(policy):
    rng,e,high=random.Random(7),seed(policy),0.
    for h in range(1440):
        for i in range(12):
            n=f'small-{i}'
            if rng.random()<.58 or not e.wallet(n).tokens:e.buy(n,rng.uniform(.03,.35),h)
            else:e.sell(n,e.wallet(n).tokens*rng.uniform(.05,.35),h)
        if h%6==0:e.sell(f'band-{(h//6)%8}',e.wallet(f'band-{(h//6)%8}').tokens*.4,h)
        if h%24==0:e.sell(f'large-{(h//24)%3}',e.wallet(f'large-{(h//24)%3}').tokens,h)
        high=max(high,e.price());e.max_drawdown=max(e.max_drawdown,1-e.price()/high)
    return e

def race(policy,reverse=False):
    e=seed(policy);names=['large-1','large-0'] if reverse else ['large-0','large-1'];a=e.sell(names[0],e.wallet(names[0]).tokens,96);b=e.sell(names[1],e.wallet(names[1]).tokens,96)
    return {'first_tokens':a[0],'first_near':a[1],'second_tokens':b[0],'second_near':b[1],'cap_net_near':e.cap_out[e.cap_bucket(96)]-e.cap_buy[e.cap_bucket(96)]}
def split_test(p):
    a,b=seed(p),seed(p);x=a.sell('band-0',a.wallet('band-0').tokens,100);kids=b.split('band-0',5);y=sum(b.sell(k,b.wallet(k).tokens,100)[1] for k in kids)
    return {'direct_tokens':x[0],'direct_near':x[1],'split_near':y,'extra_near':y-x[1],'split_cost_near':.035}
def round_trip(p,replenish=False):
    e=seed(p);x=e.buy('attacker',12,100)
    if replenish:e.buy('replenisher',30,100)
    sold,got=e.sell('attacker',x,100);return {'near_spent':12.,'near_received':got,'net':got-12,'sold_tokens':sold,'cap_net_near':e.cap_out[e.cap_bucket(100)]-e.cap_buy[e.cap_bucket(100)]}
def creator_exit(p,snipers=False):
    e=seed(p);start=e.wallet('creator').tokens
    for h in range(1440):
        if snipers and h%8==0:e.buy(f'sniper-{h}',1,h)
        e.sell('creator',e.wallet('creator').tokens,h)
        if e.wallet('creator').tokens<1:break
    return {'start_tokens':start,'remaining_tokens':e.wallet('creator').tokens,'days':h/24}
def early_exit(p,share):
    e=seed(p);total=sum(w.marked for w in e.wallets.values());f=share*SUPPLY/total
    for w in e.wallets.values():
        if w.marked:w.marked*=f;w.tokens=w.marked
    w=e.wallet('early-0')
    for h in range(1440):
        e.sell(w.name,w.tokens,h)
        if w.marked<1:break
    return {'early_supply_share':share,'holder_days':h/24,'remaining_marked_tokens':w.marked}
def base(p):return {'label':'simulation','n':p.n,'t':p.threshold,'brake':'off' if p.brake is None else p.brake,'penalty':p.penalty,'creator_fee_share':p.creator_fee_share,'window_hours':p.window}
def rows(p,only=None):
    b=base(p);e=normal(p);r={1:dict(b,test='1_normal_brake',brake_trips=e.brakes,max_drawdown=e.max_drawdown,final_price=e.price()),2:dict(b,test='2_first_seller_race',**race(p)),3:dict(b,test='3_cap_race_reversed',**race(p,True)),4:dict(b,test='4_small_holder_drawdown',max_drawdown=e.max_drawdown,final_price=e.price(),penalty_near=e.penalties),5:dict(b,test='5_brake_penalty',brakes=e.brakes,penalty_near=e.penalties),6:dict(b,test='6_penalty_level',penalty_near=e.penalties),7:dict(b,test='7_band_split',**split_test(p)),8:dict(b,test='8_cap_round_trip',**round_trip(p)),9:dict(b,test='9_buy_raise_cap',**round_trip(p,True)),10:dict(b,test='10_creator_exit',**creator_exit(p,False))};r[10].update({f'sniper_{k}':v for k,v in creator_exit(p,True).items()});out=[r[i] for i in sorted(only or r)];return out+([dict(b,test='11_early_supply_exit',**early_exit(p,x)) for x in (.09,.23,.41)] if not only or 11 in only else [])
def write(name,rows):
    cols=list(dict.fromkeys(k for r in rows for k in r));path=ROOT/'results'/name;path.parent.mkdir(exist_ok=True)
    with path.open('w',newline='') as f:o=csv.DictWriter(f,fieldnames=cols,lineterminator='\n');o.writeheader();o.writerows(rows)
def main():
    a=argparse.ArgumentParser();a.add_argument('--checkpoint',action='store_true');args=a.parse_args()
    if args.checkpoint: result=[r for s in (0.,.3) for r in rows(Policy(17,.02,None,.1,s),{1,4})]
    else:
        result=[r for n in (5,17,50) for d in (None,.1,.2,.3) for pen in (.1,.2,.3) for s in (0.,.3) for r in rows(Policy(n,.02,d,pen,s))]
        for t in (.01,.05):result+=rows(Policy(17,t,None,.1))
        for w in (1/6,24):result+=rows(Policy(17,.02,None,.1,0.,w))
    for i in range(1,12):write(f'test-{i}.csv',[r for r in result if r['test'].startswith(f'{i}_')])
if __name__=='__main__':main()
