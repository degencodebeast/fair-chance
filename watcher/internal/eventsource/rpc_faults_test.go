package eventsource_test

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"fairchance/watcher/internal/eventsource"
)

func TestAnRPCServerFaultIsTransientAndOtherSetupAnswersAreNot(t *testing.T) {
	cases := map[int]error{500: eventsource.ErrTransient, 502: eventsource.ErrTransient, 503: eventsource.ErrTransient, 504: eventsource.ErrTransient,
		401: eventsource.ErrRPCWrongSetup, 403: eventsource.ErrRPCForbidden, 404: eventsource.ErrRPCWrongSetup}
	for status, want := range cases {
		node := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(status) }))
		_, err := eventsource.ReadPreBuyViews(context.Background(), node.URL, "bag.factory.testnet")
		node.Close()
		if !errors.Is(err, want) {
			t.Errorf("status %d: err %v, want %v", status, err, want)
		}
		if (want == eventsource.ErrTransient) == (errors.Is(err, eventsource.ErrRPCForbidden) || errors.Is(err, eventsource.ErrRPCWrongSetup)) {
			t.Errorf("status %d: wrong class: %v", status, err)
		}
	}
}

func TestAnRPCTransportFaultIsTransientWithoutTheURL(t *testing.T) {
	node := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {}))
	url := node.URL
	node.Close()
	_, err := eventsource.ReadPreBuyViews(context.Background(), url, "bag.factory.testnet")
	if !errors.Is(err, eventsource.ErrTransient) || strings.Contains(err.Error(), url) {
		t.Fatalf("err %v", err)
	}
}
