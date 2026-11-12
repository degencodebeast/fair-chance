# NEAR launch replay: data provenance and collection method

## Recommendation

Use the NEAR Public Lakehouse in BigQuery as the reproducible historical
source, then verify the selected launches and the first pool/swap transaction
against an archival RPC. This avoids treating an explorer UI or token-list API
as a source of truth, while keeping the replay practical for 10–20 launches.

The Lakehouse is produced from NEAR's S3 chain data and publishes the
`crypto_near_mainnet_us` BigQuery dataset. Its source repository documents
blocks, chunks, transactions, execution outcomes, receipt details, receipt
origin, receipt actions, and a receipts view. A chunk contains the chain's
transactions and receipts; `receipt_origin` ties a receipt to the transaction
that created it. [NEAR Public Lakehouse README](https://github.com/near/near-public-lakehouse#data-available)

## Replay rules

1. Define a launch as the first successful DEX pool-creation receipt for a
   newly traded FT token. Record the DEX contract(s), pool-creation method(s),
   and token contract in versioned configuration.
2. Use that receipt's block timestamp, receipt ID, originating transaction
   hash, block height/hash, and DEX contract as the immutable launch anchor.
3. Decode successful swap/add-liquidity receipt arguments or logs; retain raw
   integer amounts, decimals, and the normalization formula. Never use float
   arithmetic for collection.
4. Sort by `(block_height, receipt_id)`, adding a DEX execution index if one
   exists. Economic actions are receipts, not merely transactions. [nearcore
   receipt specification](https://github.com/near/nearcore/blob/master/docs/RuntimeSpec/Receipts.md)
5. Verify each selected launch with archival RPC and mark missing records
   `unverified`, rather than filling them in. [NEAR archival-node
   documentation](https://near-nodes.io/archival/run-archival-node)

## Discovery-query shape

```sql
-- Pseudocode: adapt fields to the current public-dataset schema.
SELECT r.block_timestamp, r.block_height, r.receipt_id, r.transaction_hash,
       ra.receipt_receiver_account_id AS dex_contract, ra.args_base64,
       r.transaction_signer_account_id
FROM `bigquery-public-data.crypto_near_mainnet_us.receipt_actions` AS ra
JOIN `bigquery-public-data.crypto_near_mainnet_us.receipts` AS r
  USING (receipt_id)
WHERE r.block_date BETWEEN @from_date AND @to_date
  AND ra.action_kind = 'FUNCTION_CALL'
  AND ra.receipt_receiver_account_id IN UNNEST(@dex_contracts)
  AND ra.method_name IN UNNEST(@pool_creation_methods);
```

Pin the date range, retain the query job ID and contract/method allow-list,
and fail closed if required schema fields change. For each coin retain token,
symbol, decimals, DEX/pool identity, launch anchor, raw receipt values,
normalized values, source query ID, archival response hash, and its explicit
inclusion/exclusion reason.

Avoid deprecated `rpc.*.near.org` and `*.pagoda.co` endpoints; NEAR directs
backend users to providers or dedicated nodes in its [RPC deprecation
notice](https://near.org/blog/deprecation-of-near-org-and-pagoda-co-rpc-endpoints).
