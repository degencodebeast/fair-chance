package alerts_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"fairchance/watcher/internal/alerts"
	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/recordedrpc"
	"fairchance/watcher/internal/telegram"
)

// A new indexer changes nothing downstream: the same coin events, read through the sandbox block
// scan and through FastNEAR (hand-written fixture), give the same events and the same alerts.
func TestTheSameEventsThroughEitherAdapterGiveTheSameAlerts(t *testing.T) {
	ctx := context.Background()
	coins := []string{"walk.factory.sandbox"}

	sandbox := recordedrpc.Serve(t, "../../../testdata/recorded-rpc-walking-path", 27)
	fastNear := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		file := map[string]string{"/v0/account": "account_walking_path.json", "/v0/transactions": "transactions_walking_path.json"}[r.URL.Path]
		raw, err := os.ReadFile(filepath.Join("../../../testdata/hand-written-fastnear-testnet", file))
		if err != nil {
			http.NotFound(w, r)
			return
		}
		w.Write(raw)
	}))
	t.Cleanup(fastNear.Close)

	writer := alerts.Writer{}
	sources := map[string]eventsource.Source{
		"sandbox block scan": eventsource.NewRPCScan(sandbox.URL),
		"FastNEAR":           eventsource.NewFastNear(fastNear.URL),
	}
	var texts [][]string
	var datas [][]string
	for name, src := range sources {
		fake := &telegram.Fake{}
		if _, _, err := alerts.SendNew(ctx, src, coins, 15, writer, fake, 1); err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		var sent []string
		for _, m := range fake.Sent {
			sent = append(sent, m.Text)
		}
		evs, _, err := src.Events(ctx, coins, 15)
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		var blobs []string
		for _, ev := range evs {
			var compact map[string]any
			_ = json.Unmarshal(ev.Data, &compact)
			b, _ := json.Marshal(map[string]any{"name": ev.Name, "block": ev.Block, "coin": ev.Coin, "data": compact})
			blobs = append(blobs, string(b))
		}
		texts, datas = append(texts, sent), append(datas, blobs)
		if len(sent) == 0 {
			t.Fatalf("%s gave no alert", name)
		}
	}
	if !equal(texts[0], texts[1]) {
		t.Errorf("alerts differ:\n%q\n%q", texts[0], texts[1])
	}
	if !equal(datas[0], datas[1]) {
		t.Errorf("events differ:\n%q\n%q", datas[0], datas[1])
	}
}

func equal(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
