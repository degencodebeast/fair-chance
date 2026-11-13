//! Option T: one creator launches a coin, 20 buyer accounts buy in blocks 0 to N, then all of
//! them try to sell. Their sells share one early-supply allowance, and the real NEAR cap fills.
//!
//! The network is a parameter. Sandbox is the default. Testnet signs real transactions, so it
//! runs only when the builder sets [`TESTNET_YES_FLAG`] to `yes` for that run.

use anyhow::{bail, Result};

/// The environment variable the builder sets to `yes` to let Option T sign on testnet.
pub const TESTNET_YES_FLAG: &str = "FC_BUILDER_SAID_YES_TO_TESTNET_SIGNING";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoNetwork {
    Sandbox,
    Testnet,
}

/// The network from the `--network` argument (none means sandbox) and the value of
/// [`TESTNET_YES_FLAG`]. Testnet needs the flag to be exactly `yes`; any other network is refused.
pub fn choose_network(
    network: Option<&str>,
    testnet_yes_flag: Option<&str>,
) -> Result<DemoNetwork> {
    match network {
        None | Some("sandbox") => Ok(DemoNetwork::Sandbox),
        Some("testnet") if testnet_yes_flag == Some("yes") => Ok(DemoNetwork::Testnet),
        Some("testnet") => bail!(
            "Option T signs real transactions on testnet; it runs there only when the builder sets {TESTNET_YES_FLAG}=yes for that run"
        ),
        Some(other) => bail!("unknown network {other:?}: use sandbox or testnet"),
    }
}
