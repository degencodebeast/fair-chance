package eventsource

import (
	"context"
	"errors"
	"fmt"
)

// RPCScan is the Source for a node with plain NEAR RPC, such as near-sandbox.
// Plain RPC cannot list past events, so it reads every block from the cursor to the final
// head: block, then its new chunks, then each transaction's receipt outcomes and their logs.
// A transaction's events are reported at the block that included the transaction.
// ErrBlocksGone means the node no longer has the blocks from the cursor on, so a scan would
// miss events. A node that has never had a block at a height answers UNKNOWN_BLOCK for it, and
// the scan skips that height; it cannot tell that from lost data without this check.
var ErrBlocksGone = errors.New("the node no longer has the blocks from this cursor")

type RPCScan struct {
	rpc *rpcClient
}

func NewRPCScan(rpcURL string) *RPCScan {
	return &RPCScan{rpc: newRPC(rpcURL)}
}

type blockView struct {
	Header struct {
		Height uint64 `json:"height"`
	} `json:"header"`
	Chunks []struct {
		ChunkHash      string `json:"chunk_hash"`
		HeightIncluded uint64 `json:"height_included"`
	} `json:"chunks"`
}

type chunkView struct {
	Transactions []struct {
		Hash     string `json:"hash"`
		SignerID string `json:"signer_id"`
	} `json:"transactions"`
}

type txStatus struct {
	ReceiptsOutcome []struct {
		ID      string `json:"id"`
		Outcome struct {
			ExecutorID string         `json:"executor_id"`
			Logs       []string       `json:"logs"`
			Status     map[string]any `json:"status"`
		} `json:"outcome"`
	} `json:"receipts_outcome"`
}

func (s *RPCScan) Events(ctx context.Context, coins []string, since Cursor) ([]Event, Cursor, error) {
	var status struct {
		SyncInfo struct {
			EarliestBlockHeight uint64 `json:"earliest_block_height"`
		} `json:"sync_info"`
	}
	if err := s.rpc.call(ctx, "status", map[string]any{}, &status); err != nil {
		return nil, since, fmt.Errorf("node status: %w", err)
	}
	if uint64(since) < status.SyncInfo.EarliestBlockHeight {
		return nil, since, fmt.Errorf("%w: cursor %d, earliest block %d", ErrBlocksGone, since, status.SyncInfo.EarliestBlockHeight)
	}
	watched := make(map[string]bool, len(coins))
	for _, c := range coins {
		watched[c] = true
	}
	var head blockView
	if err := s.rpc.call(ctx, "block", map[string]any{"finality": "final"}, &head); err != nil {
		return nil, since, fmt.Errorf("final head: %w", err)
	}
	var out []Event
	seen := map[string]bool{}
	// On an error at height h, keep only the events of blocks before h, so the caller can
	// retry from h without sending an event twice.
	fail := func(h uint64, err error) ([]Event, Cursor, error) {
		keep := out[:0]
		for _, ev := range out {
			if ev.Block < h {
				keep = append(keep, ev)
			}
		}
		return keep, Cursor(h), err
	}
	for h := uint64(since); h <= head.Header.Height; h++ {
		var block blockView
		err := s.rpc.call(ctx, "block", map[string]any{"block_id": h}, &block)
		if errors.Is(err, errNoBlockAtHeight) {
			continue
		}
		if err != nil {
			return fail(h, fmt.Errorf("block %d: %w", h, err))
		}
		for _, ch := range block.Chunks {
			if ch.HeightIncluded != h {
				continue
			}
			var chunk chunkView
			if err := s.rpc.call(ctx, "chunk", map[string]any{"chunk_id": ch.ChunkHash}, &chunk); err != nil {
				return fail(h, fmt.Errorf("chunk %s: %w", ch.ChunkHash, err))
			}
			for _, tx := range chunk.Transactions {
				var st txStatus
				params := map[string]any{"tx_hash": tx.Hash, "sender_account_id": tx.SignerID, "wait_until": "FINAL"}
				if err := s.rpc.call(ctx, "EXPERIMENTAL_tx_status", params, &st); err != nil {
					return fail(h, fmt.Errorf("tx %s: %w", tx.Hash, err))
				}
				for _, r := range st.ReceiptsOutcome {
					if !watched[r.Outcome.ExecutorID] || seen[r.ID] || !succeeded(r.Outcome.Status) {
						continue
					}
					seen[r.ID] = true
					for _, line := range r.Outcome.Logs {
						if ev, ok := parseLog(line); ok {
							ev.Coin, ev.ReceiptID, ev.Block = r.Outcome.ExecutorID, r.ID, h
							out = append(out, ev)
						}
					}
				}
			}
		}
	}
	return out, Cursor(head.Header.Height + 1), nil
}

// succeeded reports a receipt outcome status of SuccessValue or SuccessReceiptId.
// Event indexers read successful receipts only; a failed receipt rolls back.
func succeeded(status map[string]any) bool {
	_, value := status["SuccessValue"]
	_, receipt := status["SuccessReceiptId"]
	return value || receipt
}
