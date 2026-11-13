//! The watcher (Go) and the page (TypeScript) start their block scans the same number of blocks
//! before `launch_block` as the sandbox margin test checks. The three constants must agree.

use fair_chance_sandbox_tests::LAUNCH_SCAN_MARGIN_BLOCKS;

const WATCHER_SOURCE: &str = include_str!("../../watcher/internal/eventsource/source.go");
const PAGE_SOURCE: &str = include_str!("../../web/src/chain.ts");

#[test]
fn the_watcher_and_the_page_use_the_same_launch_scan_margin() {
    let go = format!("const LaunchScanMarginBlocks = {LAUNCH_SCAN_MARGIN_BLOCKS}");
    let ts = format!("export const LAUNCH_SCAN_MARGIN_BLOCKS = {LAUNCH_SCAN_MARGIN_BLOCKS};");
    assert!(
        WATCHER_SOURCE.contains(&go),
        "watcher/internal/eventsource/source.go has no line {go:?}"
    );
    assert!(
        PAGE_SOURCE.contains(&ts),
        "web/src/chain.ts has no line {ts:?}"
    );
}
