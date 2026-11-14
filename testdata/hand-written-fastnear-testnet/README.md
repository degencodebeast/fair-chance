# Hand-written FastNEAR testnet fixtures

Written by hand in the response shape of the FastNEAR transactions API (`/v0/account` and
`/v0/transactions`). They were not recorded from the live service. The two event log lines are the
ones the sandbox walking path recorded, so the same events reach the alerts through either adapter.
Field names the watcher reads: `account_txs[].transaction_hash`, `tx_block_height`, `resume_token`;
`transactions[].transaction.hash`, `transactions[].receipts[].receipt.receipt_id`, `receipt.receiver_id`,
`execution_outcome.outcome.logs`, `execution_outcome.outcome.status`.

`receipts[].execution_outcome.block_height` is the block where the receipt ran. The live service
sends it; the watcher gives each event that block. Here it is 22 for the buy receipt (its
transaction is in block 21) and 28 for the sell receipt (its transaction is in block 27), the same
blocks the sandbox block scan reads.
