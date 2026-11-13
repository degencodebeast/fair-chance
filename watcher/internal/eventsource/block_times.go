package eventsource

import (
	"context"
	"fmt"
	"strconv"
)

// BlockTime is one block's height and its header timestamp in nanoseconds.
type BlockTime struct {
	Height uint64
	Ns     uint64
}

type blockHeader struct {
	Header struct {
		Height      uint64 `json:"height"`
		TimestampNs string `json:"timestamp_nanosec"`
	} `json:"header"`
}

func (b blockHeader) time() (BlockTime, error) {
	ns, err := strconv.ParseUint(b.Header.TimestampNs, 10, 64)
	if err != nil {
		return BlockTime{}, fmt.Errorf("block %d timestamp %q: %w", b.Header.Height, b.Header.TimestampNs, err)
	}
	return BlockTime{Height: b.Header.Height, Ns: ns}, nil
}

// BlockTimeAt reads the block at a height, or the final head when height is 0.
func BlockTimeAt(ctx context.Context, rpcURL string, height uint64) (BlockTime, error) {
	params := map[string]any{"finality": "final"}
	if height != 0 {
		params = map[string]any{"block_id": height}
	}
	var b blockHeader
	if err := newRPC(rpcURL).call(ctx, "block", params, &b); err != nil {
		return BlockTime{}, fmt.Errorf("block %d: %w", height, err)
	}
	return b.time()
}

// ReceiptBlockTime finds the block that holds a receipt's outcome: the transaction status gives
// the outcome's block hash, and the block gives its height and time.
func ReceiptBlockTime(ctx context.Context, rpcURL, txHash, signer, receiptID string) (BlockTime, error) {
	rpc := newRPC(rpcURL)
	var st struct {
		ReceiptsOutcome []struct {
			ID        string `json:"id"`
			BlockHash string `json:"block_hash"`
		} `json:"receipts_outcome"`
	}
	params := map[string]any{"tx_hash": txHash, "sender_account_id": signer, "wait_until": "FINAL"}
	if err := rpc.call(ctx, "EXPERIMENTAL_tx_status", params, &st); err != nil {
		return BlockTime{}, fmt.Errorf("tx %s: %w", txHash, err)
	}
	for _, r := range st.ReceiptsOutcome {
		if r.ID == receiptID {
			var b blockHeader
			if err := rpc.call(ctx, "block", map[string]any{"block_id": r.BlockHash}, &b); err != nil {
				return BlockTime{}, fmt.Errorf("block %s: %w", r.BlockHash, err)
			}
			return b.time()
		}
	}
	return BlockTime{}, fmt.Errorf("tx %s has no receipt %s", txHash, receiptID)
}
