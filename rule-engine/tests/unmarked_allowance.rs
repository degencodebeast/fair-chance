//! The unmarked allowance (ADR 0004) through the engine interface: one allowance per account per
//! 1 h window on the fixed grid from launch, sized by the token reserve at the first action of
//! the window (before that action), so that one allowance moves the price down by at most
//! p = 2%. `used` is net tokens on window totals, max(0, sold - bought), with no carry-over.
//!
//! Expected allowances are worked out outside this code: the largest q with
//! (y / (y + q))^2 >= 0.98 for token reserve y, that is q = floor(y / sqrt(0.98)) - y.

use std::collections::BTreeMap;

use rule_engine::{
    apply, launch, quote_sell, view_account, AccountView, Action, Applied, At, CoinState, Event,
    FcSell, FixedValues, Holder, Holders, LaunchRequest,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const LAUNCH_NS: u64 = 100_000_000_000;
const HOUR_NS: u64 = 3_600_000_000_000;

/// Allowance at the full launch reserve (1e27 base units): 1e27 / sqrt(0.98) - 1e27, rounded
/// down (Python `Decimal`, 60 digits).
const ALLOWANCE_AT_LAUNCH: u128 = 10_152_544_552_210_749_144_063_374;

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
                coin: "coin.factory.test",
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

    fn act(&mut self, action: Action, at: At) -> Applied {
        let applied = apply(&self.state, self.holders(&action), &action, at).expect("action");
        self.state = applied.state.clone();
        self.ledger.extend(applied.holders.clone());
        applied
    }

    fn buy(&mut self, account: &str, near_in: u128, at: At) -> u128 {
        let applied = self.act(
            Action::Buy {
                account: account.into(),
                near_in,
                min_tokens_out: 0,
            },
            at,
        );
        match applied.events.iter().find_map(|e| match e {
            Event::FcBuy(b) => Some(b.tokens_out),
            _ => None,
        }) {
            Some(tokens) => tokens,
            None => panic!("no fc_buy"),
        }
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
        sell_event(&applied)
    }

    fn account(&self, account: &str, at: At) -> AccountView {
        view_account(&self.state, self.ledger.get(account), at)
    }
}

fn sell_event(applied: &Applied) -> FcSell {
    applied
        .events
        .iter()
        .find_map(|e| match e {
            Event::FcSell(s) => Some(s.clone()),
            _ => None,
        })
        .expect("fc_sell")
}

#[test]
fn a_fresh_coin_gives_every_account_one_allowance_of_two_percent_price_drop() {
    let coin = Coin::launched();
    let view = coin.account("alice.test", after(10));
    assert_eq!(view.unmarked.allowance, ALLOWANCE_AT_LAUNCH);
    assert_eq!(view.unmarked.used, 0);
    assert_eq!(view.unmarked.left, ALLOWANCE_AT_LAUNCH);
    assert_eq!(view.unmarked.window_end_ns, LAUNCH_NS + HOUR_NS);
}
