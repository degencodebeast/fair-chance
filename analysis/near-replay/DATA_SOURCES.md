# NEAR launch replay: data provenance and collection method

## Recommendation

Use public launchpad indexes for bounded discovery, FastNEAR's public
transaction index to locate signed launch calls, and a public archival RPC
to verify every receipt that an index exposes. The replay does not require a
Google account, BigQuery, an API key, a wallet, or a paid endpoint.

## Replay rules

1. Define a launch as the signed launch transaction for a newly traded FT
   token. Record its signer, transaction hash, DEX contract(s), and token
   contract in versioned configuration.
2. Use that transaction's block timestamp and height as the immutable launch
   anchor; record the first pool-swap receipt separately for pool provenance.
3. Decode successful swap/add-liquidity receipt arguments or logs; retain raw
   integer amounts, decimals, and the normalization formula. Never use float
   arithmetic for collection.
4. Sort by `(block_height, receipt_id)`, adding a DEX execution index if one
   exists. Economic actions are receipts, not merely transactions. [nearcore
   receipt specification](https://github.com/near/nearcore/blob/master/docs/RuntimeSpec/Receipts.md)
5. Verify each selected launch with archival RPC and mark missing records
   `unverified`, rather than filling them in. [NEAR archival-node
   documentation](https://near-nodes.io/archival/run-archival-node)

Pin the launch-list response, pool-trade responses, FastNEAR launch-transaction
responses, and archival-RPC receipt
responses in the committed fixture set. For each coin retain token, symbol,
decimals, DEX/pool identity, launch anchor, raw receipt values, normalized
values, and an explicit inclusion/exclusion reason.

Avoid deprecated `rpc.*.near.org` and `*.pagoda.co` endpoints; NEAR directs
backend users to providers or dedicated nodes in its [RPC deprecation
notice](https://near.org/blog/deprecation-of-near-org-and-pagoda-co-rpc-endpoints).
