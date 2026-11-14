package eventsource_test

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
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

func TestFastNearTreatsAServerFaultAsTransientAndKeepsTheCursor(t *testing.T) {
	for _, status := range []int{500, 502, 503, 504} {
		srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(status) }))
		_, next, err := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{"c.testnet"}, 7)
		srv.Close()
		if !errors.Is(err, eventsource.ErrTransient) || errors.Is(err, eventsource.ErrUnexpectedResponse) || next != 7 {
			t.Errorf("status %d: err %v next %d; want a transient error and cursor 7", status, err, next)
		}
		if !strings.Contains(err.Error(), strconv.Itoa(status)) {
			t.Errorf("status %d: the error text %q does not name the status", status, err)
		}
	}
}

func TestFastNearTreatsATransportFaultAsTransientWithoutTheURL(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {}))
	url := srv.URL
	srv.Close() // nothing listens now: connection refused
	_, next, err := eventsource.NewFastNear(url).Events(context.Background(), []string{"c.testnet"}, 7)
	if !errors.Is(err, eventsource.ErrTransient) || next != 7 {
		t.Fatalf("err %v next %d", err, next)
	}
	if strings.Contains(err.Error(), url) {
		t.Fatalf("the error text holds the URL: %v", err)
	}
}

func TestFastNearTreatsATimeoutAsTransient(t *testing.T) {
	release := make(chan struct{})
	srv := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) { <-release }))
	defer srv.Close()
	defer close(release)
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()
	_, next, err := eventsource.NewFastNear(srv.URL).Events(ctx, []string{"c.testnet"}, 7)
	if !errors.Is(err, eventsource.ErrTransient) || next != 7 {
		t.Fatalf("err %v next %d", err, next)
	}
}

func TestFastNearStopsOnAnswersThatMeanAWrongSetup(t *testing.T) {
	for _, status := range []int{http.StatusUnauthorized, http.StatusNotFound} {
		srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(status) }))
		_, _, err := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{"c.testnet"}, 7)
		srv.Close()
		if !errors.Is(err, eventsource.ErrUnexpectedResponse) || errors.Is(err, eventsource.ErrTransient) {
			t.Errorf("status %d: err %v", status, err)
		}
	}
}

func TestFastNearEventsCarryTheirTransactionHashAndSigner(t *testing.T) {
	var bodies []map[string]any
	srv := fastNearServer(t, &bodies)

	evs, _, err := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{"walk.factory.sandbox"}, 15)
	if err != nil {
		t.Fatal(err)
	}
	if len(evs) != 2 {
		t.Fatalf("events %+v", evs)
	}
	if evs[0].TxHash != "33Y9QyfznjCoTjkkxXFoygiYaPxiG47FBR2NFGE2eGiA" || evs[0].TxSigner != "alice.sandbox" {
		t.Fatalf("buy tx %q signer %q", evs[0].TxHash, evs[0].TxSigner)
	}
	if evs[1].TxHash != "Fy6sboDgwfKMDsMTqGaURTDFTX9eRinb4JKdKoGmvRSt" || evs[1].TxSigner != "alice.sandbox" {
		t.Fatalf("sell tx %q signer %q", evs[1].TxHash, evs[1].TxSigner)
	}
}
