//! The burst guard: before the launch, the run estimates the block (counted from the launch block)
//! where the last pre-signed transaction of the burst lands, and refuses to start when that block
//! is after the early window. The expected blocks are worked out by hand in each test.

use fair_chance_sandbox_tests::burst::{check_burst_timing, check_launch_answer, BurstTiming};

/// Testnet as measured on 14 Nov: 0.5812 s blocks, 3.45 s per call on the public RPC.
fn testnet_today() -> BurstTiming {
    BurstTiming {
        block_s: 0.5812,
        send_s: 3.45,
        launch_tail_blocks: 3,
        early_sell_gap_s: 1.2,
    }
}

#[test]
fn the_testnet_timing_of_today_lands_in_block_14() {
    // 3 tail blocks + ceil((3.45 + 1.2) / 0.5812 = 8.0007) = 9 blocks + 2 (include, execute).
    assert_eq!(check_burst_timing(&testnet_today(), 17).unwrap(), 14);
}

#[test]
fn a_slow_rpc_is_refused_before_anything_is_signed() {
    let slow = BurstTiming {
        send_s: 7.0,
        ..testnet_today()
    };
    // 3 + ceil(8.2 / 0.5812 = 14.1) = 15 + 2 = 20, after block 17.
    let error = check_burst_timing(&slow, 17).unwrap_err().to_string();
    assert!(
        error.contains("block 20") && error.contains("17") && error.contains("nothing was signed"),
        "{error}"
    );
}

#[test]
fn the_last_block_of_the_window_still_passes() {
    // 3 + ceil((5.0 + 1.2) / 0.5 = 12.4) = 13 + 2 = 18 is refused; 4.7 s gives
    // 3 + ceil(11.8) = 12 + 2 = 17, the last block of the window.
    let edge = BurstTiming {
        block_s: 0.5,
        send_s: 4.7,
        ..testnet_today()
    };
    assert_eq!(check_burst_timing(&edge, 17).unwrap(), 17);
    let over = BurstTiming {
        send_s: 5.0,
        ..edge
    };
    assert!(check_burst_timing(&over, 17).is_err());
}

#[test]
fn a_block_time_that_is_not_positive_is_refused() {
    let broken = BurstTiming {
        block_s: 0.0,
        ..testnet_today()
    };
    assert!(check_burst_timing(&broken, 17).is_err());
}

#[test]
fn a_launch_answer_that_comes_back_late_stops_the_burst() {
    // The estimate is block 14 of 17, so 3 blocks are spare. The launch answer may take one send
    // call plus (include + new + tail + spare) = 1 + 1 + 3 + 3 = 8 blocks: 3.45 + 8 x 0.5812 =
    // 8.0996 s.
    let timing = testnet_today();
    assert!(check_launch_answer(&timing, 17, 8.0).is_ok());
    let error = check_launch_answer(&timing, 17, 8.2)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("8.2") && error.contains("no buy was sent"),
        "{error}"
    );
}
