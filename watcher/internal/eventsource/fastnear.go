package eventsource

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"slices"
	"strconv"
	"strings"
	"time"
)

// FastNear is the Source for NEAR testnet, read through the FastNEAR transactions API.
// It is read only. It does not retry itself: a 403 or a wrong-setup answer is an error that stops the
// caller, a 429 or a short fault (ErrTransient) is an error the caller waits out, and the cursor stays. The public endpoint needs no key.
//
// Request shapes follow the FastNEAR read check: POST /v0/account lists the transactions of an
// account that logged events, POST /v0/transactions gives up to 20 of them with their receipt logs.
type FastNear struct {
	baseURL string
	http    *http.Client
}

var (
	ErrForbidden   = errors.New("FastNEAR answered 403: stop and report")
	ErrRateLimited = errors.New("the service answered 429: wait before the next read")
	// ErrRPCForbidden is a 403 from the NEAR RPC: stop and report, no retry.
	ErrRPCForbidden = errors.New("the RPC answered 403: stop and report")
	// ErrRPCWrongSetup is any other client-side refusal from the RPC (401, 404, 400 ...): the setup is
	// wrong, so reads stop and the builder checks the source.
	ErrRPCWrongSetup = errors.New("the RPC refused the read, which means a wrong setup: stop and report")
	// ErrUnexpectedResponse is an answer that means a wrong setup (a status such as 401 or 404, a bad URL,
	// a body the watcher cannot read). Stop and report.
	ErrUnexpectedResponse = errors.New("FastNEAR gave an answer the watcher does not know: stop and report")
	// ErrTransient is a short fault: an HTTP 5xx, a timeout or a transport error. Wait and read again
	// from the same cursor.
	ErrTransient = errors.New("short service fault: wait and read again")
)

// RateLimitedError is a 429. It is-a ErrRateLimited. RetryAfter is the wait FastNEAR asked for, or 0.
type RateLimitedError struct{ RetryAfter time.Duration }

func (e *RateLimitedError) Error() string        { return ErrRateLimited.Error() }
func (e *RateLimitedError) Is(target error) bool { return target == ErrRateLimited }

const (
	accountPageSize = 100
	accountMaxPages = 10
	transactionsMax = 20 // hashes per /v0/transactions call
)

func NewFastNear(baseURL string) *FastNear {
	return &FastNear{baseURL: baseURL, http: &http.Client{Timeout: 30 * time.Second}}
}

type accountTx struct {
	Hash   string `json:"transaction_hash"`
	Height uint64 `json:"tx_block_height"`
}

type fastNearTx struct {
	Transaction struct {
		Hash     string `json:"hash"`
		SignerID string `json:"signer_id"`
	} `json:"transaction"`
	Receipts []struct {
		Receipt struct {
			ReceiptID  string `json:"receipt_id"`
			ReceiverID string `json:"receiver_id"`
		} `json:"receipt"`
		ExecutionOutcome struct {
			BlockHeight uint64 `json:"block_height"` // the block where the receipt ran
			Outcome     struct {
				Logs   []string       `json:"logs"`
				Status map[string]any `json:"status"`
			} `json:"outcome"`
		} `json:"execution_outcome"`
	} `json:"receipts"`
}

func (f *FastNear) Events(ctx context.Context, coins []string, since Cursor) ([]Event, Cursor, error) {
	var out []Event
	seen := map[string]bool{}
	next := since
	for _, coin := range coins {
		txs, err := f.accountTxsSince(ctx, coin, since)
		if err != nil {
			return nil, since, err
		}
		// Chain order: oldest first.
		slices.SortStableFunc(txs, func(a, b accountTx) int { return int(int64(a.Height) - int64(b.Height)) })
		for start := 0; start < len(txs); start += transactionsMax {
			batch := txs[start:min(start+transactionsMax, len(txs))]
			hashes := make([]string, len(batch))
			for i, tx := range batch {
				hashes[i] = tx.Hash
			}
			var res struct {
				Transactions []fastNearTx `json:"transactions"`
			}
			if err := f.post(ctx, "/v0/transactions", map[string]any{"tx_hashes": hashes}, &res); err != nil {
				return nil, since, err
			}
			heights := map[string]uint64{}
			for _, tx := range batch {
				heights[tx.Hash] = tx.Height
			}
			for _, tx := range res.Transactions {
				if height, asked := heights[tx.Transaction.Hash]; asked {
					out = append(out, eventsOfTx(tx, coin, height, seen)...)
				}
			}
		}
		for _, tx := range txs {
			next = max(next, Cursor(tx.Height+1))
		}
	}
	slices.SortStableFunc(out, func(a, b Event) int { return int(int64(a.Block) - int64(b.Block)) })
	return out, next, nil
}

func eventsOfTx(tx fastNearTx, coin string, height uint64, seen map[string]bool) []Event {
	var out []Event
	for _, r := range tx.Receipts {
		if r.Receipt.ReceiverID != coin || seen[r.Receipt.ReceiptID] || !succeeded(r.ExecutionOutcome.Outcome.Status) {
			continue
		}
		// The event belongs to the block where its receipt ran, which can be later than the block
		// of the transaction. An answer without that height falls back to the transaction's block.
		block := height
		if r.ExecutionOutcome.BlockHeight != 0 {
			block = r.ExecutionOutcome.BlockHeight
		}
		for _, line := range r.ExecutionOutcome.Outcome.Logs {
			if ev, ok := parseLog(line); ok {
				ev.Coin, ev.ReceiptID, ev.Block = coin, r.Receipt.ReceiptID, block
				ev.TxHash, ev.TxSigner = tx.Transaction.Hash, tx.Transaction.SignerID
				out = append(out, ev)
			}
		}
		seen[r.Receipt.ReceiptID] = true
	}
	return out
}

// accountTxsSince pages through the newest-first list until it reaches a transaction below the cursor.
func (f *FastNear) accountTxsSince(ctx context.Context, coin string, since Cursor) ([]accountTx, error) {
	var kept []accountTx
	token := ""
	for page := 0; page < accountMaxPages; page++ {
		body := map[string]any{"account_id": coin, "desc": true, "is_event_log": true, "is_success": true, "limit": accountPageSize}
		if token != "" {
			body["resume_token"] = token
		}
		var res struct {
			AccountTxs  []accountTx `json:"account_txs"`
			ResumeToken *string     `json:"resume_token"`
		}
		if err := f.post(ctx, "/v0/account", body, &res); err != nil {
			return nil, err
		}
		reachedOld := false
		for _, tx := range res.AccountTxs {
			if Cursor(tx.Height) < since {
				reachedOld = true
				continue
			}
			kept = append(kept, tx)
		}
		if reachedOld || res.ResumeToken == nil || *res.ResumeToken == "" || len(res.AccountTxs) == 0 {
			return kept, nil
		}
		token = *res.ResumeToken
	}
	return nil, fmt.Errorf("more than %d pages of transactions for %s since block %d", accountMaxPages, coin, since)
}

func (f *FastNear) post(ctx context.Context, path string, body any, out any) error {
	raw, err := json.Marshal(body)
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, f.baseURL+path, bytes.NewReader(raw))
	if err != nil {
		return fmt.Errorf("bad FastNEAR URL: %w", ErrUnexpectedResponse)
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := f.http.Do(req)
	if err != nil {
		return fmt.Errorf("%s: %v: %w", path, withoutURL(err), ErrTransient)
	}
	defer resp.Body.Close()
	switch resp.StatusCode {
	case http.StatusOK:
	case http.StatusForbidden:
		return fmt.Errorf("%s: %w", path, ErrForbidden)
	case http.StatusTooManyRequests:
		return fmt.Errorf("%s: %w", path, &RateLimitedError{RetryAfter: retryAfter(resp.Header.Get("Retry-After"))})
	default:
		if resp.StatusCode >= 500 {
			return fmt.Errorf("%s: HTTP %d: %w", path, resp.StatusCode, ErrTransient)
		}
		return fmt.Errorf("%s: HTTP %d: %w", path, resp.StatusCode, ErrUnexpectedResponse)
	}
	if err := json.NewDecoder(resp.Body).Decode(out); err != nil {
		return fmt.Errorf("%s: %v: %w", path, err, ErrUnexpectedResponse)
	}
	return nil
}

// retryAfter reads a Retry-After header given in seconds or as an HTTP date. A missing or bad value is 0.
func retryAfter(header string) time.Duration {
	if secs, err := strconv.Atoi(strings.TrimSpace(header)); err == nil && secs > 0 {
		return time.Duration(secs) * time.Second
	}
	if at, err := http.ParseTime(header); err == nil {
		return max(0, time.Until(at))
	}
	return 0
}
