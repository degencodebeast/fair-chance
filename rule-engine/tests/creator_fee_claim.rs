//! The creator fee share and its claim (ADR 0010), through the engine interface: `apply`,
//! `settle_creator_claim` and the views.
//!
//! s = 30% of the normal swap fee goes to a creator balance outside the pool's real NEAR. The
//! coin keeps the lowest real NEAR of each 1 h window (read after each action; a new window
//! starts at the real NEAR carried from the last action; window 0 starts at 0) and
//! `depth_ok_since`. The creator claims with 1 yoctoNEAR at least 24 h after `depth_ok_since`,
//! while the lowest real NEAR of every window since then, the current one included, is at least
//! X = 10 NEAR.

use std::collections::BTreeMap;

use rule_engine::{
    apply, launch, settle_creator_claim, view_cap, view_creator_fees, view_pool, Action, Applied,
    At, CoinState, CreatorFeesView, Event, FcSell, FixedValues, Holder, Holders, LaunchRequest,
    Refusal,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const LAUNCH_NS: u64 = 100_000_000_000;
const HOUR_NS: u64 = 3_600_000_000_000;
const DAY_NS: u64 = 24 * HOUR_NS;
const CREATOR: &str = "creator.test";
const STRANGER: &str = "stranger.test";
/// Real NEAR a 0.5 NEAR buy leaves in the pool: 0.5 less 30% of its 0.005 NEAR fee.
const HALF_NEAR_BUY_REAL_NEAR: u128 = 498_500_000_000_000_000_000_000;

/// A moment `seconds` into 1 h window `window` after launch. Blocks are past the early window.
fn at(window: u64, seconds: u64) -> At {
    let ns = window * HOUR_NS + seconds * 1_000_000_000;
    At {
        block: 100 + 18 + ns / 1_000_000_000,
        ns: LAUNCH_NS + ns,
    }
}

fn window_start_ns(window: u64) -> u64 {
    LAUNCH_NS + window * HOUR_NS
}

/// One coin and its whole ledger, driven only through `apply` and the views.
#[derive(Clone)]
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
                creator: CREATOR,
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

    fn holders(&self, accounts: &[String]) -> Holders {
        accounts
            .iter()
            .filter_map(|a| self.ledger.get(a).map(|h| (a.clone(), h.clone())))
            .collect()
    }

    fn try_act(&mut self, action: Action, at: At) -> Result<Applied, Refusal> {
        let applied = apply(&self.state, self.holders(&action.accounts()), &action, at)?;
        self.state = applied.state.clone();
        self.ledger.extend(applied.holders.clone());
        Ok(applied)
    }

    fn buy(&mut self, account: &str, near_in: u128, at: At) {
        self.try_act(
            Action::Buy {
                account: account.into(),
                near_in,
                min_tokens_out: 0,
            },
            at,
        )
        .expect("buy");
    }

    fn sell_all(&mut self, account: &str, at: At) -> FcSell {
        let amount = self.ledger[account].balance;
        let applied = self
            .try_act(
                Action::Sell {
                    account: account.into(),
                    amount,
                    min_near_out: 0,
                    attached: 1,
                },
                at,
            )
            .expect("sell");
        applied
            .events
            .iter()
            .find_map(|e| match e {
                Event::FcSell(s) => Some(s.clone()),
                _ => None,
            })
            .expect("fc_sell")
    }

    fn claim_by(&mut self, account: &str, attached: u128, at: At) -> Result<Applied, Refusal> {
        self.try_act(
            Action::Claim {
                account: account.into(),
                attached,
            },
            at,
        )
    }

    fn claim(&mut self, at: At) -> Result<Applied, Refusal> {
        self.claim_by(CREATOR, 1, at)
    }

    fn fees(&self, at: At) -> CreatorFeesView {
        view_creator_fees(&self.state, at)
    }

    fn real_near(&self, at: At) -> u128 {
        view_pool(&self.state, at).real_near
    }
}

/// Twenty-one small accounts buy 0.5 NEAR each in window `window`: about 10.47 real NEAR, just
/// above X. Each bag is below one allowance, so each is small.
fn coin_just_above_the_minimum(window: u64) -> Coin {
    let mut coin = Coin::launched();
    for i in 0..21 {
        coin.buy(&format!("friend-{i}.test"), ONE_NEAR / 2, at(window, i));
    }
    assert_eq!(coin.real_near(at(window, 30)), 21 * HALF_NEAR_BUY_REAL_NEAR);
    coin
}

/// One friend sells its whole bag, so real NEAR dips below X, and a new account buys 0.5 NEAR
/// back right after, in the same window.
fn dip_below_the_minimum_and_back(coin: &mut Coin, window: u64, name: &str) {
    coin.sell_all("friend-0.test", at(window, 10));
    assert!(coin.real_near(at(window, 10)) < 10 * ONE_NEAR);
    coin.buy(name, ONE_NEAR / 2, at(window, 20));
    assert!(coin.real_near(at(window, 20)) >= 10 * ONE_NEAR);
}

#[test]
fn window_0_starts_its_lowest_real_near_at_0_so_the_24_h_count_starts_at_window_1() {
    let coin = coin_just_above_the_minimum(0);
    let in_window_0 = coin.fees(at(0, 100));
    assert_eq!(in_window_0.depth_ok_since_ns, None);
    assert_eq!(in_window_0.claimable_at_ns, None);
    assert!(!in_window_0.claimable_now);

    let in_window_1 = coin.fees(at(1, 0));
    assert_eq!(in_window_1.depth_ok_since_ns, Some(window_start_ns(1)));
    assert_eq!(
        in_window_1.claimable_at_ns,
        Some(window_start_ns(1) + DAY_NS)
    );
    assert!(!in_window_1.claimable_now);
}

#[test]
fn windows_with_no_actions_keep_the_carried_real_near_and_count_toward_the_24_h() {
    let mut coin = coin_just_above_the_minimum(0);
    assert!(!coin.fees(at(24, 3_599)).claimable_now);
    assert_eq!(
        coin.claim(at(24, 3_599)).unwrap_err().to_string(),
        "FC_CLAIM_NOT_READY"
    );
    let ready = coin.fees(at(25, 0));
    assert!(ready.claimable_now);
    assert_eq!(ready.depth_ok_since_ns, Some(window_start_ns(1)));
    assert!(coin.claim(at(25, 0)).is_ok());
}

#[test]
fn a_window_that_never_reaches_the_minimum_keeps_the_count_off() {
    let mut coin = Coin::launched();
    coin.buy("friend-0.test", 5 * ONE_NEAR, at(0, 0));
    for window in [1, 30, 100] {
        let fees = coin.fees(at(window, 0));
        assert_eq!(fees.depth_ok_since_ns, None);
        assert_eq!(fees.claimable_at_ns, None);
    }
    assert_eq!(
        coin.claim(at(100, 0)).unwrap_err().to_string(),
        "FC_CLAIM_NOT_READY"
    );
}

#[test]
fn a_dip_below_the_minimum_in_a_past_window_restarts_the_24_h() {
    let mut coin = coin_just_above_the_minimum(0);
    dip_below_the_minimum_and_back(&mut coin, 5, "friend-21.test");
    // Window 5 ends with real NEAR above X, but its lowest value was below it.
    assert_eq!(coin.fees(at(5, 30)).depth_ok_since_ns, None);
    let fees = coin.fees(at(6, 0));
    assert_eq!(fees.depth_ok_since_ns, Some(window_start_ns(6)));
    assert_eq!(fees.claimable_at_ns, Some(window_start_ns(6) + DAY_NS));
    assert_eq!(
        coin.claim(at(29, 3_599)).unwrap_err().to_string(),
        "FC_CLAIM_NOT_READY"
    );
    assert!(coin.claim(at(30, 0)).is_ok());
}

#[test]
fn a_dip_below_the_minimum_in_the_current_window_refuses_the_claim() {
    let mut coin = coin_just_above_the_minimum(0);
    assert!(coin.fees(at(40, 0)).claimable_now);
    dip_below_the_minimum_and_back(&mut coin, 40, "friend-21.test");
    let fees = coin.fees(at(40, 30));
    assert!(!fees.claimable_now);
    assert_eq!(fees.depth_ok_since_ns, None);
    assert_eq!(
        coin.claim(at(40, 30)).unwrap_err().to_string(),
        "FC_CLAIM_NOT_READY"
    );
    assert_eq!(
        coin.fees(at(41, 0)).depth_ok_since_ns,
        Some(window_start_ns(41))
    );
}

#[test]
fn only_the_creator_can_claim_and_only_with_exactly_one_yocto() {
    let mut coin = coin_just_above_the_minimum(0);
    let ready = at(25, 0);
    assert_eq!(
        coin.claim_by(STRANGER, 1, ready).unwrap_err().to_string(),
        "FC_NOT_CREATOR"
    );
    for attached in [0, 2] {
        assert_eq!(
            coin.claim_by(CREATOR, attached, ready)
                .unwrap_err()
                .to_string(),
            "FC_ONE_YOCTO"
        );
    }
    assert!(coin.claim_by(CREATOR, 1, ready).is_ok());
}

#[test]
fn a_claim_with_a_zero_balance_is_refused_as_not_ready() {
    let mut coin = coin_just_above_the_minimum(0);
    coin.claim(at(25, 0)).expect("first claim");
    let fees = coin.fees(at(25, 1));
    assert_eq!(fees.balance, 0);
    assert!(!fees.claimable_now);
    assert_eq!(
        coin.claim(at(25, 1)).unwrap_err().to_string(),
        "FC_CLAIM_NOT_READY"
    );
}

#[test]
fn a_claim_pays_the_whole_balance_and_logs_nothing_until_the_transfer_succeeds() {
    let mut coin = coin_just_above_the_minimum(0);
    let ready = at(25, 0);
    // 21 buys of 0.5 NEAR: 30% of each 0.005 NEAR fee.
    let balance = 21 * 1_500_000_000_000_000_000_000;
    assert_eq!(coin.fees(ready).balance, balance);

    let claimed = coin.claim(ready).expect("claim");
    let payout = claimed.payout.expect("payout");
    assert_eq!(payout.account, CREATOR);
    assert_eq!(payout.amount, balance);
    assert!(claimed.events.is_empty());
    assert_eq!(coin.fees(ready).balance, 0);
    assert_eq!(coin.fees(ready).claimed_total, 0);

    let settled = settle_creator_claim(&coin.state, balance, window_start_ns(1), true, ready);
    let fees = view_creator_fees(&settled.state, ready);
    assert_eq!(fees.balance, 0);
    assert_eq!(fees.claimed_total, balance);
    assert_eq!(settled.events.len(), 1);
    let line = settled.events[0].log_line();
    let event: serde_json::Value =
        serde_json::from_str(line.strip_prefix("EVENT_JSON:").unwrap()).unwrap();
    assert_eq!(event["standard"], "fair_chance");
    assert_eq!(event["event"], "fc_creator_claim");
    assert_eq!(
        event["data"][0],
        serde_json::json!({
            "creator": CREATOR,
            "amount": balance.to_string(),
            "depth_ok_since_ns": window_start_ns(1).to_string(),
            "block": ready.block,
        })
    );
}

#[test]
fn a_failed_claim_transfer_restores_the_balance_and_logs_nothing() {
    let mut coin = coin_just_above_the_minimum(0);
    let ready = at(25, 0);
    let balance = coin.fees(ready).balance;
    coin.claim(ready).expect("claim");
    // Fees earned between the claim and its callback stay too.
    coin.buy("late.test", ONE_NEAR / 2, at(25, 1));
    let settled = settle_creator_claim(&coin.state, balance, window_start_ns(1), false, at(25, 2));
    assert!(settled.events.is_empty());
    let fees = view_creator_fees(&settled.state, at(25, 2));
    assert_eq!(fees.balance, balance + 1_500_000_000_000_000_000_000);
    assert_eq!(fees.claimed_total, 0);
}

#[test]
fn a_claim_never_lowers_the_real_near_or_the_cap() {
    let mut coin = coin_just_above_the_minimum(0);
    let ready = at(25, 0);
    let pool_before = view_pool(&coin.state, ready);
    let cap_before = view_cap(&coin.state, ready);
    coin.claim(ready).expect("claim");
    let pool_after = view_pool(&coin.state, ready);
    assert_eq!(pool_after.real_near, pool_before.real_near);
    assert_eq!(pool_after.near_reserve, pool_before.near_reserve);
    assert_eq!(pool_after.creator_fee_balance, 0);
    assert_eq!(view_cap(&coin.state, ready), cap_before);
}

#[test]
fn the_penalty_and_the_early_fee_extra_never_reach_the_creator_balance() {
    let mut coin = Coin::launched();
    // Block 0 after launch: a 50% early fee on 1 NEAR, but the creator gets 30% of 1% only.
    coin.try_act(
        Action::Buy {
            account: "early.test".into(),
            near_in: ONE_NEAR,
            min_tokens_out: 0,
        },
        At {
            block: 100,
            ns: LAUNCH_NS,
        },
    )
    .expect("early buy");
    assert_eq!(coin.fees(at(0, 1)).balance, 3_000_000_000_000_000_000_000);
    // A band account (1.5 NEAR, above one allowance) sells all of it next window, after forty
    // 1 NEAR buys deepen the pool: it pays the penalty, and the creator gets 30% of the fee only.
    coin.buy("band.test", 3 * ONE_NEAR / 2, at(0, 10));
    for i in 0..40 {
        coin.buy(&format!("depth-{i}.test"), ONE_NEAR, at(1, 0));
    }
    let before = coin.fees(at(1, 1)).balance;
    let sold = coin.sell_all("band.test", at(1, 1));
    assert_eq!(sold.class, "band");
    assert!(sold.penalty_near > 0);
    assert_eq!(sold.creator_fee_near, sold.fee_near * 3 / 10);
    assert_eq!(coin.fees(at(1, 1)).balance, before + sold.creator_fee_near);
}

/// Paper case c: a friend group puts in just over X = 10 NEAR to fake the depth. The NEAR must
/// stay in the pool for the full 24 h: one friend that takes its NEAR back after 12 h restarts
/// the count. Known limit (ADR 0010): a group that holds for 24 h does get the claim.
#[test]
fn paper_case_c_a_friend_group_must_hold_the_minimum_for_the_full_24_h() {
    let mut withdraws = coin_just_above_the_minimum(0);
    withdraws.sell_all("friend-0.test", at(13, 0));
    assert!(withdraws.real_near(at(13, 0)) < 10 * ONE_NEAR);
    assert_eq!(withdraws.fees(at(25, 0)).depth_ok_since_ns, None);
    assert_eq!(
        withdraws.claim(at(25, 0)).unwrap_err().to_string(),
        "FC_CLAIM_NOT_READY"
    );

    let mut holds = coin_just_above_the_minimum(0);
    assert!(holds.claim(at(25, 0)).is_ok());
}
