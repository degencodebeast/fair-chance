# Recorded sandbox RPC responses

Recorded from one walking-path run on near-sandbox 2.13.4: the factory launched
`walk.factory.sandbox` at block 15, `alice.sandbox` bought at block 21 and sold half at block 27.
Block 22 has a chunk with no transactions. Heights with no file answer `UNKNOWN_BLOCK`.

Files are named by request: `block_<height>.json`, `chunk_<chunk hash>.json`,
`tx_<tx hash>.json` (`EXPERIMENTAL_tx_status`), `query_<method>.json` (coin view), `status.json`
(node status, from a later run of the same path; its `earliest_block_height` is 0).
The Go watcher tests and the web tests both serve these files from a fake RPC endpoint.
