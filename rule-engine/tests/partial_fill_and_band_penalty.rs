//! Sells above the allowance (ADR 0004), through the engine interface: `apply` and the views.
//!
//! A large account's sell above its allowance fills in part, and the rest stays in the wallet.
//! A band account's first allowance in each window always fills; the part above it fills too and
//! pays a penalty of `penalty_bps` (10%) of its NEAR output, which stays in the pool. A limited
//! sell never panics for the limit.
//!
//! Worked numbers come from a separate Python model of the pool (exact integers): 100 NEAR
//! virtual, 1e27 tokens, 1% swap fee rounded up, NEAR out rounded down. The part above the first
//! allowance gets the NEAR the whole sell pays minus the NEAR the first-allowance part alone
//! would pay, both before the swap fee; its NEAR output is that amount minus its own swap fee
//! (rounded up), and the penalty is 10% of it, rounded up.

use std::collections::BTreeMap;

use rule_engine::{
    apply, launch, quote_sell, view_account, AccountView, Action, Applied, At, CoinState, Event,
    FcSell, FixedValues, Holder, Holders, LaunchRequest, Payout,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const ONE_TOKEN: u128 = 1_000_000_000_000_000_000;
const LAUNCH_NS: u64 = 100_000_000_000;
const HOUR_S: u64 = 3_600;

const BAND: &str = "band.test";
const LARGE: &str = "large.test";
const SMALL: &str = "small.test";
const OTHER: &str = "other.test";

/// The band account's 1.5 NEAR buy at launch: about 1.46% of supply.
const BAND_TOKENS: u128 = 14_632_704_340_542_937_379_908_360;
/// The allowance in window 1 (token reserve 985_367_295_659_457_062_620_091_640 after the buy).
const ALLOWANCE_WINDOW_1: u128 = 10_003_985_369_474_059_362_529_806;
/// The floor in window 1: 0.1% (10 bps) of that token reserve, rounded down.
const FLOOR_WINDOW_1: u128 = 985_367_295_659_457_062_620_091;

/// Seconds after launch, as a block time.
fn after(seconds: u64) -> At {
    At {
        block: 100 + seconds,
        ns: LAUNCH_NS + seconds * 1_000_000_000,
    }
}

/// The first second of window 1.
fn window_1() -> At {
    after(HOUR_S)
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

    fn buy(&mut self, account: &str, near_in: u128, at: At) {
        self.act(
            Action::Buy {
                account: account.into(),
                near_in,
                min_tokens_out: 0,
            },
            at,
        );
    }

    fn sell_applied(&mut self, account: &str, amount: u128, at: At) -> Applied {
        self.act(
            Action::Sell {
                account: account.into(),
                amount,
                min_near_out: 0,
                attached: 1,
            },
            at,
        )
    }

    fn sell(&mut self, account: &str, amount: u128, at: At) -> FcSell {
        sell_event(&self.sell_applied(account, amount, at))
    }

    fn send(&mut self, from: &str, to: &str, amount: u128, at: At) {
        self.act(
            Action::Send {
                from: from.into(),
                to: to.into(),
                amount,
                memo: None,
            },
            at,
        );
    }

    fn quote(&self, account: &str, amount: u128, at: At) -> rule_engine::SellQuote {
        let holders: Holders = self
            .ledger
            .get(account)
            .map(|h| (account.to_string(), h.clone()))
            .into_iter()
            .collect();
        quote_sell(&self.state, holders, account, amount, at).expect("quote")
    }

    fn account(&self, account: &str, at: At) -> AccountView {
        view_account(&self.state, self.ledger.get(account), at)
    }

    fn balance(&self, account: &str) -> u128 {
        self.ledger.get(account).map_or(0, |h| h.balance)
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

/// BAND buys 1.5 NEAR at launch (band: above one allowance, below 2%).
fn coin_with_a_band_holder() -> Coin {
    let mut coin = Coin::launched();
    coin.buy(BAND, 1_500 * ONE_NEAR / 1_000, after(10));
    assert_eq!(coin.balance(BAND), BAND_TOKENS);
    coin
}

#[test]
fn a_band_sell_inside_its_first_allowance_fills_in_full_with_no_penalty() {
    let mut coin = coin_with_a_band_holder();
    assert_eq!(coin.account(BAND, window_1()).class, "band");

    let sold = coin.sell(BAND, ALLOWANCE_WINDOW_1, window_1());

    assert_eq!(sold.class, "band");
    assert_eq!(sold.tokens_sold, ALLOWANCE_WINDOW_1);
    assert_eq!(sold.fill, "full");
    assert_eq!(sold.limit_reason, None);
    assert_eq!(sold.penalty_near, 0);
    assert_eq!(sold.fee_near, 10_201_263_933_915_967_826_401);
    assert_eq!(sold.near_out, 1_009_925_129_457_680_814_813_615);
}

#[test]
fn a_band_sell_above_its_first_allowance_fills_in_full_and_pays_ten_percent_of_the_part_above_to_the_pool(
) {
    let mut coin = coin_with_a_band_holder();
    let real_near_before = coin.state.real_near;

    let applied = coin.sell_applied(BAND, BAND_TOKENS, window_1());
    let sold = sell_event(&applied);

    assert_eq!(sold.class, "band");
    assert_eq!(sold.tokens_asked, BAND_TOKENS);
    assert_eq!(sold.tokens_sold, BAND_TOKENS);
    assert_eq!(sold.fill, "full");
    assert_eq!(sold.limit_reason, None);
    assert_eq!(sold.fee_near, 14_852_194_905_651_081_440_607);
    assert_eq!(sold.penalty_near, 46_044_216_620_177_624_780_648);
    assert_eq!(sold.near_out, 1_424_323_079_039_279_437_839_443);
    // The seller gets near_out; the fee and the penalty stay in the pool's real NEAR.
    assert_eq!(
        applied.payout,
        Some(Payout {
            account: BAND.to_string(),
            amount: sold.near_out
        })
    );
    assert_eq!(coin.state.real_near, real_near_before - sold.near_out);
    assert_eq!(coin.balance(BAND), 0);
}
