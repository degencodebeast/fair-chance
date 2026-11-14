//! The checks on a coin the app launched, made before any buy is sent: the coin runs the known
//! code, has the fixed early window, and its creator is not the runner (not the funder and not one
//! of the funder's sub-accounts, which the runner signs for). Then the detection check: a coin
//! found too late for the burst plan gets no buy.

use fair_chance_sandbox_tests::app_coin::{
    check_app_coin, check_detection, expected_buy_block, AppCoinFacts,
};
use fair_chance_sandbox_tests::burst::BurstTiming;

const KNOWN: &str = "6Ld5DFTJ1BYc6uzb6YYvdK3yAMHdfBsvrwNEhsKE9er3";
const OTHER: &str = "11111111111111111111111111111111";

fn funder() -> near_api::types::AccountId {
    "fairchance.testnet".parse().unwrap()
}

fn good() -> AppCoinFacts {
    AppCoinFacts {
        account_code_hash: KNOWN.to_string(),
        rules_code_hash: KNOWN.to_string(),
        creator: "builder-wallet.testnet".to_string(),
        early_blocks: 17,
    }
}

#[test]
fn a_coin_with_the_known_code_and_another_creator_passes() {
    check_app_coin(&good(), &funder(), KNOWN, 17).unwrap();
}

#[test]
fn a_coin_the_runner_could_have_launched_is_refused() {
    for creator in [
        "fairchance.testnet",
        "cabal3-creator.fairchance.testnet",
        "x.y.fairchance.testnet",
    ] {
        let facts = AppCoinFacts {
            creator: creator.to_string(),
            ..good()
        };
        let refused = check_app_coin(&facts, &funder(), KNOWN, 17)
            .expect_err(creator)
            .to_string();
        assert!(
            refused.contains(creator) && refused.contains("no buy was sent"),
            "{refused}"
        );
    }
}

#[test]
fn a_creator_that_only_ends_like_the_funder_is_not_the_funder() {
    let facts = AppCoinFacts {
        creator: "notfairchance.testnet".to_string(),
        ..good()
    };
    check_app_coin(&facts, &funder(), KNOWN, 17).unwrap();
}

#[test]
fn unknown_code_on_the_account_or_in_the_rules_is_refused() {
    for facts in [
        AppCoinFacts {
            account_code_hash: OTHER.to_string(),
            ..good()
        },
        AppCoinFacts {
            rules_code_hash: OTHER.to_string(),
            ..good()
        },
    ] {
        let refused = check_app_coin(&facts, &funder(), KNOWN, 17)
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains(OTHER) && refused.contains("no buy was sent"),
            "{refused}"
        );
    }
}

#[test]
fn another_early_window_is_refused() {
    let facts = AppCoinFacts {
        early_blocks: 5,
        ..good()
    };
    assert!(check_app_coin(&facts, &funder(), KNOWN, 17).is_err());
}

/// The C2 numbers: 0.577 s per block, 3.72 s per send call, 1.2 s gap. With 2 detect blocks the
/// guard puts the last early sell in block 2 + ceil(4.92 / 0.577) + 2 = 13, so 4 blocks are spare.
fn c2_timing(detect_blocks: u64) -> BurstTiming {
    BurstTiming {
        block_s: 0.577,
        send_s: 3.72,
        launch_tail_blocks: detect_blocks,
        early_sell_gap_s: 1.2,
    }
}

#[test]
fn a_coin_found_within_the_planned_blocks_and_the_spare_ones_gets_the_burst() {
    for lag in 0..=6 {
        check_detection(&c2_timing(2), 17, lag).unwrap();
    }
}

#[test]
fn a_coin_found_later_than_the_plan_allows_gets_no_buy() {
    let refused = check_detection(&c2_timing(2), 17, 7)
        .unwrap_err()
        .to_string();
    assert!(refused.contains("no buy was sent"), "{refused}");
}

/// The block the buys should land in, for the detection log line: the find, then one send call,
/// then one block to get in and one to run on the coin. C2 timing: 6.45 blocks of send round up to
/// 7, so a find 2 blocks after the launch block gives 2 + 7 + 2 = 11.
#[test]
fn the_expected_buy_block_is_the_find_plus_one_send_call_plus_two_blocks() {
    assert_eq!(expected_buy_block(&c2_timing(2), 2), 11);
    assert_eq!(expected_buy_block(&c2_timing(2), 0), 9);
    let no_delay = BurstTiming {
        send_s: 0.0,
        ..c2_timing(2)
    };
    assert_eq!(expected_buy_block(&no_delay, 1), 3);
}
