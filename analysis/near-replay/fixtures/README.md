# Frozen public responses

Each filename is the SHA-256 digest of the exact request used by
`replay.py`: a Nearly URL is hashed directly; an RPC request is serialized as
compact JSON, then hashed with an `rpc-` prefix. The runner looks here before
its ignored `.cache/`, so a clean checkout replays this exact data without a
network request.

The files are public launch-list, launch, trade, token-view, pool-view, and
receipt responses collected on 2026-11-12. They contain no credentials,
signatures, or private account data.
