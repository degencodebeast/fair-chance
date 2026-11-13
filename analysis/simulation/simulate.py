#!/usr/bin/env python3
"""Deterministic synthetic exercise of the Issue #2 sell rules."""
from __future__ import annotations
import argparse, csv, json, random
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
    cap_counted: dict = field(default_factory=lambda: defaultdict(float)); cap_noncounted: dict = field(default_factory=lambda: defaultdict(float)); cap_buy: dict = field(default_factory=lambda: defaultdict(float))
    snaps: dict = field(default_factory=dict); reserve_snaps: dict = field(default_factory=dict); prices: dict = field(default_factory=dict); brakes: int = 0; penalties: float = 0.; max_drawdown: float = 0.
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
        self.snapshot(hour, block); marked = block <= self.policy.n or creator or self.wallet(name).creator
        extra = .09 * max(0., 1 - block / self.policy.n) if block <= self.policy.n else 0.
        estimate = self.pool.token - self.pool.token * self.pool.near / (self.pool.near + near * (1-FEE-extra))
        group = creator or self.wallet(name).creator
        if group and self.creator_tokens() + estimate > .04 * SUPPLY: return 0.
        out = self.pool.buy(near, self.policy.creator_fee_share, extra); w = self.wallet(name, group)
        w.tokens += out; w.marked += out if marked or group else 0.; w.ever_large |= w.tokens >= self.policy.threshold * SUPPLY
        self.bought[name, self.bucket(hour)] += out
        if not group: self.cap_buy[name, self.cap_bucket(hour)] += near
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
        for _ in range(32):
            mid=(lo+hi)/2; gross=min(self.pool.gross(mid),self.pool.real); payout=gross*(1-FEE-penalty); creator=gross*FEE*self.policy.creator_fee_share
            if self.cap_used(hour,name,payout) <= CAP*(self.pool.real-payout-creator): lo=mid
            else: hi=mid
        return lo
    def floor_tokens(self, name, hour, marked):
        reserve=self.reserve_snapshot(hour)
        if marked:
            amounts,total,_=self.snaps[self.bucket(hour,True)]; return FLOOR*reserve*amounts.get(name,0.)/total if total else 0.
        return FLOOR*reserve
    def sell(self, name, wanted, hour, block=10_000):
        self.snapshot(hour, block); w=self.wallet(name); wanted=min(wanted,w.tokens); marked=min(wanted,w.marked); unmarked=wanted-marked
        if block<=self.policy.n: marked=0.
        total=payout=0.
        if marked:
            k=self.bucket(hour,True); amounts,supply,reserve=self.snaps[k]; allowance=ALLOWANCE*reserve*amounts.get(name,0.)/supply if supply else 0.
            wanted_marked=min(marked,max(0.,allowance-self.sold[name,k])); amount=self.cap_fill(name,wanted_marked,hour,0.)
            if amount < wanted_marked: amount=min(wanted_marked,max(amount,self.floor_tokens(name,hour,True)))
            paid,fee=self.pool.sell(amount,self.policy.creator_fee_share); self.sold[name,k]+=amount; self.cap_counted[name,self.cap_bucket(hour)]+=paid; self.penalties+=fee; total+=amount;payout+=paid
        if unmarked:
            k,ck=self.bucket(hour),self.cap_bucket(hour); room=max(0.,ALLOWANCE*self.reserve_snapshot(hour)+self.bought[name,k]-self.sold[name,k])
            base=min(unmarked,room); extra=0. if w.ever_large else max(0.,unmarked-base)
            if w.ever_large:
                wanted_base=base; base=self.cap_fill(name,base,hour,0.)
                if base < wanted_base: base=min(wanted_base,max(base,self.floor_tokens(name,hour,False)))
            start=self.prices.setdefault(ck,self.price())
            if extra and self.policy.brake is not None and 1-self.price()/start>=self.policy.brake: self.brakes+=1;extra=0.
            extra=self.cap_fill(name,extra,hour,self.policy.penalty)
            for amount,penalty,counted in ((base,0.,w.ever_large),(extra,self.policy.penalty,True)):
                if amount:
                    paid,fee=self.pool.sell(amount,self.policy.creator_fee_share,penalty); self.sold[name,k]+=amount
                    if counted:self.cap_counted[name,ck]+=paid
                    else:self.cap_noncounted[name,ck]+=paid
                    self.penalties+=fee;total+=amount;payout+=paid
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
    creator_near=(e.pool.token*e.pool.near/(e.pool.token-.04*SUPPLY)-e.pool.near)/(1-FEE-.09)
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
    b=base(p);e=normal(p);r={1:dict(b,test='1_normal_brake',brake_trips=e.brakes,max_drawdown=e.max_drawdown,final_price=e.price()),2:dict(b,test='2_first_seller_race',**race(p)),3:dict(b,test='3_cap_race_reversed',**race(p,True)),4:dict(b,test='4_small_holder_drawdown',max_drawdown=e.max_drawdown,final_price=e.price(),penalty_near=e.penalties),5:dict(b,test='5_brake_penalty',brakes=e.brakes,penalty_near=e.penalties),6:dict(b,test='6_penalty_level',penalty_near=e.penalties),7:dict(b,test='7_band_split',**split_test(p)),8:dict(b,test='8_cap_round_trip',**round_trip(p)),9:dict(b,test='9_buy_raise_cap',**round_trip(p,True))}
    if not only or 10 in only:
        r[10]=dict(b,test='10_creator_exit',**creator_exit(p,False));r[10].update({f'sniper_{k}':v for k,v in creator_exit(p,True).items()})
    out=[r[i] for i in sorted(only or r) if i in r]
    return out+([dict(b,test='11_early_supply_exit',**early_exit(p,x)) for x in (.09,.23,.41)] if not only or 11 in only else [])
def write(name,rows,directory=None):
    cols=list(dict.fromkeys(k for r in rows for k in r));path=(directory or ROOT/'results')/name;path.parent.mkdir(parents=True,exist_ok=True)
    with path.open('w',newline='') as f:o=csv.DictWriter(f,fieldnames=cols,lineterminator='\n');o.writeheader();o.writerows(rows)
def unit(value, scale): return str(int(round(value*scale)))
def cap_cases():
    """Small deterministic vectors for a contract implementation (decimal math)."""
    cases=[]
    for share in (0.,.3):
        p=Policy(17,.02,None,.1,share); e=Engine(p,Pool(token=SUPPLY,near=110.,real=10.)); e.wallet('large').tokens=500_000_000.;e.wallet('large').ever_large=True
        actions=[]
        def sell(account, amount, second=0, block=100):
            filled,paid=e.sell(account,amount,second/3600,block)
            actions.append({'account':account,'kind':'sell','amount_token_base':unit(amount,1e18),'time_s':second,'block':block,'result':{'tokens_filled_base':unit(filled,1e18),'near_out_yocto':unit(paid,1e24),'cap_used_yocto':unit(e.cap_used(second/3600),1e24),'refusal_code':None if filled else 'FC_CAP_FULL'}})
        sell('large',500_000_000.)
        cases.append({'name':f'large-post-sell-cap-s-{int(share*100)}','start_state':{'real_near_yocto':unit(10,1e24),'virtual_near_yocto':unit(100,1e24),'token_reserve_base':unit(SUPPLY,1e18),'creator_fee_share_bps':int(share*10000)},'actions':actions})
        e=Engine(p,Pool(token=SUPPLY,near=110.,real=10.)); e.wallet('same').tokens=500_000_000.;e.wallet('same').ever_large=True; actions=[]
        bought=e.buy('same',100,0); actions.append({'account':'same','kind':'buy','amount_near_yocto':unit(100,1e24),'time_s':0,'block':100,'result':{'tokens_filled_base':unit(bought,1e18),'cap_used_yocto':unit(e.cap_used(0),1e24),'refusal_code':None}})
        sell('same',e.wallet('same').tokens,1)
        cases.append({'name':f'same-account-round-trip-s-{int(share*100)}','start_state':{'real_near_yocto':unit(10,1e24),'virtual_near_yocto':unit(100,1e24),'token_reserve_base':unit(SUPPLY,1e18),'creator_fee_share_bps':int(share*10000)},'actions':actions})
    (ROOT/'results'/'cap-cases.json').write_text(json.dumps({'version':'1.0.0-rc.4','math':'decimal floating-point simulation; amounts are serialized as integer base units','cases':cases},indent=2)+'\n')
def main():
    a=argparse.ArgumentParser();a.add_argument('--checkpoint',action='store_true');a.add_argument('--shard',type=int);a.add_argument('--shards',type=int);a.add_argument('--merge',action='store_true');args=a.parse_args()
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
