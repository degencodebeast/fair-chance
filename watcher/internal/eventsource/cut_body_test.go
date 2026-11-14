package eventsource_test

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"

	"fairchance/watcher/internal/eventsource"
)

func TestFastNearTreatsACutResponseBodyAsTransient(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Length", "500")
		w.WriteHeader(200)
		w.Write([]byte(`{"account_txs":[`))
		w.(http.Flusher).Flush()
		c, _, _ := w.(http.Hijacker).Hijack()
		c.Close()
	}))
	defer srv.Close()
	_, _, err := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{"a.testnet"}, 1)
	if !errors.Is(err, eventsource.ErrTransient) || errors.Is(err, eventsource.ErrUnexpectedResponse) {
		t.Fatalf("err %v: a cut body is a short fault, not a wrong setup", err)
	}
}

func TestFastNearKeepsABadJSONBodyAsUnexpected(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.Write([]byte(`not json`)) }))
	defer srv.Close()
	_, _, err := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{"a.testnet"}, 1)
	if !errors.Is(err, eventsource.ErrUnexpectedResponse) || errors.Is(err, eventsource.ErrTransient) {
		t.Fatalf("err %v", err)
	}
}

func TestAnRPCCutBodyIsTransientAndTimeoutStatusesAreTransient(t *testing.T) {
	cut := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Length", "500")
		w.WriteHeader(200)
		w.Write([]byte(`{"result":`))
		w.(http.Flusher).Flush()
		c, _, _ := w.(http.Hijacker).Hijack()
		c.Close()
	}))
	defer cut.Close()
	if _, err := eventsource.ReadPreBuyViews(context.Background(), cut.URL, "bag.factory.testnet"); !errors.Is(err, eventsource.ErrTransient) {
		t.Errorf("cut body: err %v", err)
	}
	for _, status := range []int{408, 425} { // 408 is nearcore's TIMEOUT_ERROR
		node := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(status) }))
		_, err := eventsource.ReadPreBuyViews(context.Background(), node.URL, "bag.factory.testnet")
		node.Close()
		if !errors.Is(err, eventsource.ErrTransient) || errors.Is(err, eventsource.ErrRPCWrongSetup) {
			t.Errorf("status %d: err %v", status, err)
		}
	}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(408) }))
	defer srv.Close()
	_, _, err := eventsource.NewFastNear(srv.URL).Events(context.Background(), []string{"a.testnet"}, 1)
	if !errors.Is(err, eventsource.ErrTransient) {
		t.Errorf("FastNEAR 408: err %v", err)
	}
}
