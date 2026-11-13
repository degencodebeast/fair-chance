//! The testnet RPCs of a run: reads go to FastNEAR first with the public RPC as fallback; each
//! signed transaction goes to the public RPC only. near-api does not retry on its own on either
//! (one try per endpoint), so the only waits are the 429 rule in [`crate::rate_limit`] and a
//! transaction is never sent twice. A URL that could hold a key is refused and never echoed.

use anyhow::{bail, Result};
use near_api::{NetworkConfig, RPCEndpoint};

/// One endpoint with one try, after the URL checks.
fn endpoint(url: &str, role: &str) -> Result<RPCEndpoint> {
    let refuse = || {
        anyhow::anyhow!(
            "the {role} RPC URL is refused: use https with no user, password, query or fragment, on testnet"
        )
    };
    let mut endpoint = RPCEndpoint::new(url.parse().map_err(|_| refuse())?);
    let u = &endpoint.url;
    let host = u.host_str().unwrap_or("");
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || host.contains("mainnet")
        || !host.contains("testnet")
    {
        return Err(refuse());
    }
    endpoint.retries = 1;
    Ok(endpoint)
}

/// The read network (FastNEAR, then the public RPC) and the send network (the public RPC only).
pub fn testnet_rpcs(read_primary: &str, public: &str) -> Result<(NetworkConfig, NetworkConfig)> {
    let primary = endpoint(read_primary, "read")?;
    let public = endpoint(public, "public")?;
    if primary.url == public.url {
        bail!("the read RPC and the public RPC are the same; give FastNEAR as the read RPC");
    }
    let mut read = NetworkConfig::from_rpc_url("testnet", public.url.clone());
    read.rpc_endpoints = vec![primary, public.clone()];
    let mut send = NetworkConfig::from_rpc_url("testnet", public.url.clone());
    send.rpc_endpoints = vec![public];
    Ok((read, send))
}
