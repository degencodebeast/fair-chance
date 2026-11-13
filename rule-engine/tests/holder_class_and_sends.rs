//! Holder class (ADR 0001) and sends (NEP-141 with the Fair Chance refusals), through the engine
//! interface: `apply` and the views.
//!
//! Class: `small` when the balance fits in one allowance, `band` above that, `large` from the
//! buy that takes the balance to T = 2% of supply (2e25 base units of 1e27) until the balance
//! is zero. Only a buy makes an account large.
//!
//! Token amounts for a buy are worked out outside this code on the launch pool (100 NEAR
//! virtual, 1e27 tokens, 1% fee): a buy of n NEAR gives about 1e27 x 0.99n / (100 + 0.99n).

use std::collections::BTreeMap;

use rule_engine::{
    apply, launch, quote_buy, view_account, view_pool, AccountView, Action, Applied, At,
    CoinState, Event, FcSell, FixedValues, Holder, Holders, LaunchRequest, Refusal,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const LAUNCH_NS: u64 = 100_000_000_000;
const HOUR_S: u64 = 3_600;
const COIN: &str = "coin.factory.test";
/// T = 2% of the 1e27 supply: the smallest large balance.
const LARGE_THRESHOLD: u128 = 20_000_000_000_000_000_000_000_000;

const BUYER: &str = "buyer.test";
const LARGE: &str = "large.test";
const SMALL: &str = "small.test";
const OTHER: &str = "other.test";

/// Seconds after launch, as a block time.
fn after(seconds: u64) -> At {
    At {
        block: 100 + seconds,
        ns: LAUNCH_NS + seconds * 1_000_000_000,
    }
}

/// One coin and its whole ledger, driven only through `apply` and the views.
struct Coin {
    state: CoinState,
    ledger: BTreeMap<String, Holder>,
}

impl Coin {
    fn launched() -> Self {
        let state = launch(
            FixedValues::fixed(),
            LaunchRequest {
                coin: COIN,
                caller: "factory.test",
                creator: "creator.test",
                code_hash: "hash",
                name: "Test Coin",
                symbol: "TEST",
                icon: None,
                description: None,
            },
            At {
                block: 100,
                ns: LAUNCH_NS,
            },
        )
        .expect("launch")
        .state;
        Self {
            state,
            ledger: BTreeMap::new(),
        }
    }

    fn holders(&self, action: &Action) -> Holders {
        action
            .accounts()
            .into_iter()
            .filter_map(|a| self.ledger.get(&a).map(|h| (a.clone(), h.clone())))
            .collect()
    }

    fn try_act(&mut self, action: Action, at: At) -> Result<Applied, Refusal> {
        let applied = apply(&self.state, self.holders(&action), &action, at)?;
        self.state = applied.state.clone();
        self.ledger.extend(applied.holders.clone());
        Ok(applied)
    }

    fn act(&mut self, action: Action, at: At) -> Applied {
        self.try_act(action, at).expect("action")
    }

    fn buy(&mut self, account: &str, near_in: u128, at: At) -> Applied {
        self.act(
            Action::Buy {
                account: account.into(),
                near_in,
                min_tokens_out: 0,
            },
            at,
        )
    }

    fn sell(&mut self, account: &str, amount: u128, at: At) -> FcSell {
        let applied = self.act(
            Action::Sell {
                account: account.into(),
                amount,
                min_near_out: 0,
                attached: 1,
            },
            at,
        );
        applied
            .events
            .iter()
            .find_map(|e| match e {
                Event::FcSell(s) => Some(s.clone()),
                _ => None,
            })
            .expect("fc_sell")
    }

    fn send(&mut self, from: &str, to: &str, amount: u128, at: At) -> Result<Applied, Refusal> {
        self.try_act(
            Action::Send {
                from: from.into(),
                to: to.into(),
                amount,
                memo: None,
            },
            at,
        )
    }

    /// NEP-145 registration: an empty record for the account.
    fn register(&mut self, account: &str) {
        self.ledger
            .entry(account.to_string())
            .or_insert_with(|| Holder::registered(0));
    }

    fn account(&self, account: &str, at: At) -> AccountView {
        view_account(&self.state, self.ledger.get(account), at)
    }

    fn balance(&self, account: &str) -> u128 {
        self.ledger.get(account).map_or(0, |h| h.balance)
    }

    fn large_holders(&self, at: At) -> u32 {
        view_pool(&self.state, at).large_holders
    }
}

fn became_large(applied: &Applied) -> Vec<rule_engine::FcBecameLarge> {
    applied
        .events
        .iter()
        .filter_map(|e| match e {
            Event::FcBecameLarge(b) => Some(b.clone()),
            _ => None,
        })
        .collect()
}

fn class_after(applied: &Applied) -> &'static str {
    applied
        .events
        .iter()
        .find_map(|e| match e {
            Event::FcBuy(b) => Some(b.class_after),
            _ => None,
        })
        .expect("fc_buy")
}

#[test]
fn paper_case_e_a_three_percent_buy_is_large_at_once_and_cannot_send() {
    let mut coin = Coin::launched();
    let at = after(10);
    let quote = quote_buy(&coin.state, Holders::new(), BUYER, 3_200 * ONE_NEAR / 1_000, at)
        .expect("quote");

    let bought = coin.buy(BUYER, 3_200 * ONE_NEAR / 1_000, at);

    let balance = coin.balance(BUYER);
    assert!(balance * 100 >= 3 * 1_000_000_000 * 10u128.pow(18));
    assert_eq!(class_after(&bought), "large");
    assert_eq!(quote.class_after, "large");
    assert!(quote.becomes_large);
    let events = became_large(&bought);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].account, BUYER);
    assert_eq!(events[0].balance, balance);
    assert_eq!(events[0].large_threshold, LARGE_THRESHOLD);
    let view = coin.account(BUYER, after(20));
    assert_eq!(view.class, "large");
    assert!(view.is_large);
    assert!(!view.can_send);
    assert_eq!(view.max_receive, None);

    coin.register(OTHER);
    let refused = coin.send(BUYER, OTHER, 1, after(20)).unwrap_err();
    assert_eq!(refused.to_string(), "FC_LARGE_CANNOT_SEND");
}
