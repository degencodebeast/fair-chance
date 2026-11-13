package eventsource_test

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"fairchance/watcher/internal/eventsource"
)

const fixtureDir = "../../../testdata/hand-written-fastnear-testnet"

// fastNearServer serves the hand-written fixtures and records the request bodies.
func fastNearServer(t *testing.T, bodies *[]map[string]any) *httptest.Server {
	t.Helper()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		_ = json.NewDecoder(r.Body).Decode(&body)
		*bodies = append(*bodies, body)
		file := map[string]string{"/v0/account": "account_walking_path.json", "/v0/transactions": "transactions_walking_path.json"}[r.URL.Path]
		if r.Method != http.MethodPost || file == "" {
			http.NotFound(w, r)
			return
		}
		raw, err := os.ReadFile(filepath.Join(fixtureDir, file))
		if err != nil {
			t.Error(err)
		}
		w.Write(raw)
	}))
	t.Cleanup(srv.Close)
	return srv
}

func TestFastNearGivesTheCoinsEventsInChainOrderWithTheNextCursor(t *testing.T) {
	var bodies []map[string]any
	srv := fastNearServer(t, &bodies)

	evs, next, err := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{"walk.factory.sandbox"}, 15)
	if err != nil {
		t.Fatal(err)
	}
	if len(evs) != 2 || evs[0].Name != "fc_buy" || evs[0].Block != 21 || evs[1].Name != "fc_sell" || evs[1].Block != 27 {
		t.Fatalf("events %+v", evs)
	}
	if evs[0].Coin != "walk.factory.sandbox" || evs[1].ReceiptID != "receiptSell1" || evs[0].Standard != "fair_chance" {
		t.Fatalf("fields %+v", evs)
	}
	if next != 28 {
		t.Fatalf("next cursor %d, want 28", next)
	}
	first := bodies[0]
	if first["account_id"] != "walk.factory.sandbox" || first["is_event_log"] != true || first["is_success"] != true {
		t.Fatalf("account request %v", first)
	}
}

func TestFastNearSkipsTransactionsBeforeTheCursor(t *testing.T) {
	var bodies []map[string]any
	srv := fastNearServer(t, &bodies)

	evs, next, err := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{"walk.factory.sandbox"}, 25)
	if err != nil || len(evs) != 1 || evs[0].Name != "fc_sell" || next != 28 {
		t.Fatalf("events %+v next %d err %v", evs, next, err)
	}
}

func TestFastNearKeepsTheCursorWhenThereIsNothingNew(t *testing.T) {
	var bodies []map[string]any
	srv := fastNearServer(t, &bodies)

	evs, next, err := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{"walk.factory.sandbox"}, 40)
	if err != nil || len(evs) != 0 || next != 40 {
		t.Fatalf("events %+v next %d err %v", evs, next, err)
	}
}

func TestFastNearStopsOnForbiddenAndReportsRateLimits(t *testing.T) {
	for status, want := range map[int]error{http.StatusForbidden: eventsource.ErrForbidden, http.StatusTooManyRequests: eventsource.ErrRateLimited} {
		calls := 0
		srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			calls++
			w.WriteHeader(status)
		}))
		_, next, err := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{"c.testnet"}, 7)
		srv.Close()
		if !errors.Is(err, want) || next != 7 || calls != 1 {
			t.Errorf("status %d: err %v next %d calls %d; want %v, 7, 1 call (no retry)", status, err, next, calls, want)
		}
	}
}

func TestFastNearReportsRetryAfterOfARateLimit(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Retry-After", "120")
		w.WriteHeader(http.StatusTooManyRequests)
	}))
	defer srv.Close()
	_, _, err := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{"c.testnet"}, 7)
	var limited *eventsource.RateLimitedError
	if !errors.As(err, &limited) || limited.RetryAfter != 120*time.Second {
		t.Fatalf("err %v", err)
	}
}

func TestFastNearTreatsAnyOtherAnswerAsUnknown(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(http.StatusBadGateway) }))
	defer srv.Close()
	_, _, err := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{"c.testnet"}, 7)
	if !errors.Is(err, eventsource.ErrUnexpectedResponse) {
		t.Fatalf("err %v", err)
	}
}
