package alerts_test

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"slices"
	"testing"

	"fairchance/watcher/internal/alerts"
	"fairchance/watcher/internal/eventsource"
)

// receiptOrderSource keeps the inclusion filter of FastNEAR and block-scan, and orders by receipt block.
type receiptOrderSource struct{ all []eventsource.Event }

func (z receiptOrderSource) Events(_ context.Context, _ []string, since eventsource.Cursor) ([]eventsource.Event, eventsource.Cursor, error) {
	var out []eventsource.Event
	for _, e := range z.all {
		if eventsource.Cursor(e.Included) >= since {
			out = append(out, e)
		}
	}
	slices.SortStableFunc(out, func(a, b eventsource.Event) int { return int(int64(a.Block) - int64(b.Block)) })
	return out, 100, nil
}

type failsFirstSend struct{ calls, ok int }

func (f *failsFirstSend) Send(context.Context, int64, string) error {
	f.calls++
	if f.calls == 1 {
		return errors.New("telegram 502")
	}
	f.ok++
	return nil
}

func TestASendFailureLosesNoAlertWhenAnEarlierIncludedTxRanLater(t *testing.T) {
	// borrow two real events, then set heights: tx1 included 10 runs at 15; tx2 included 12 runs at 13.
	base := aSellEvent(t)
	e1, e2 := base, base
	e1.ReceiptID, e1.Included, e1.Block = "r1", 10, 15
	e2.ReceiptID, e2.Included, e2.Block = "r2", 12, 13
	s := &failsFirstSend{}
	src := receiptOrderSource{[]eventsource.Event{e1, e2}}
	// The source gives e2 (ran at 13) before e1 (ran at 15). The first send fails, so e2 is unsent while e1 is not yet sent either.
	_, next, err := alerts.SendNew(context.Background(), src, []string{coin}, 10, alerts.Writer{}, s, 1)
	if err == nil {
		t.Fatal("the first send must fail")
	}
	if _, _, err := alerts.SendNew(context.Background(), src, []string{coin}, next, alerts.Writer{}, s, 1); err != nil {
		t.Fatal(err)
	}
	if s.calls < 3 || s.ok != 2 { // a repeat is fine, a lost alert is not
		t.Fatalf("delivered %d of 2 alerts in %d sends: one is lost", s.ok, s.calls)
	}
}

func aSellEvent(t *testing.T) eventsource.Event {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		file := map[string]string{"/v0/account": "account_walking_path.json", "/v0/transactions": "transactions_walking_path.json"}[r.URL.Path]
		raw, _ := os.ReadFile(filepath.Join("../../../testdata/hand-written-fastnear-testnet", file))
		w.Write(raw)
	}))
	defer srv.Close()
	evs, _, _ := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{coin}, 15)
	for _, e := range evs {
		if _, ok := (alerts.Writer{}).Sentence(e); ok {
			return e
		}
	}
	t.Fatal("no event")
	return eventsource.Event{}
}
