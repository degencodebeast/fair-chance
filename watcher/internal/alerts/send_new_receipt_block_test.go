package alerts_test

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"fairchance/watcher/internal/alerts"
	"fairchance/watcher/internal/eventsource"
)

type sendFailsOnce struct{ calls, ok int }

func (f *sendFailsOnce) Send(_ context.Context, _ int64, _ string) error {
	f.calls++
	if f.calls == 1 {
		return errors.New("telegram 502")
	}
	f.ok++
	return nil
}

func TestASendFailureLosesNoAlertWhenTheReceiptRanInALaterBlock(t *testing.T) {
	dir := "../../../testdata/hand-written-fastnear-testnet"
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		file := map[string]string{"/v0/account": "account_walking_path.json", "/v0/transactions": "transactions_walking_path.json"}[r.URL.Path]
		raw, _ := os.ReadFile(filepath.Join(dir, file))
		if r.URL.Path == "/v0/transactions" {
			var doc map[string]any
			json.Unmarshal(raw, &doc)
			for _, tx := range doc["transactions"].([]any) {
				for _, rc := range tx.(map[string]any)["receipts"].([]any) {
					rc.(map[string]any)["execution_outcome"].(map[string]any)["block_height"] = 1000
				}
			}
			raw, _ = json.Marshal(doc)
		}
		w.Write(raw)
	}))
	defer srv.Close()
	src := eventsource.NewFastNear(srv.URL)
	s := &sendFailsOnce{}
	ctx := context.Background()
	evs, _, _ := src.Events(ctx, []string{coin}, 15)
	n := 0
	for _, e := range evs {
		if _, ok := (alerts.Writer{}).Sentence(e); ok {
			n++
		}
	}
	sent, next, err := alerts.SendNew(ctx, src, []string{coin}, 15, alerts.Writer{}, s, 42)
	_ = err
	_ = sent
	_, _, _ = alerts.SendNew(ctx, src, []string{coin}, next, alerts.Writer{}, s, 42)
	if s.ok != n {
		t.Fatalf("LOST: delivered %d of %d alerts", s.ok, n)
	}
}
