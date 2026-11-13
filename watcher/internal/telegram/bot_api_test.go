package telegram_test

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"fairchance/watcher/internal/telegram"
)

// The Bot API is served by a local test server. The token here is a made-up test string.
func botAPIServer(t *testing.T, requests *[]string) *httptest.Server {
	t.Helper()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		*requests = append(*requests, r.URL.Path+" "+string(body))
		switch {
		case strings.HasSuffix(r.URL.Path, "/getUpdates"):
			w.Write([]byte(`{"ok":true,"result":[{"update_id":10,"message":{"chat":{"id":42},"text":"/watch moth.factory.testnet"}},{"update_id":11,"edited_message":{"chat":{"id":42},"text":"x"}}]}`))
		case strings.HasSuffix(r.URL.Path, "/sendMessage"):
			w.Write([]byte(`{"ok":true,"result":{}}`))
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(srv.Close)
	return srv
}

func TestBotAPIReadsTextMessagesAndAdvancesTheOffset(t *testing.T) {
	var requests []string
	srv := botAPIServer(t, &requests)
	api := telegram.NewBotAPI("test-token", srv.URL)

	msgs, next, err := api.Updates(context.Background(), 0)
	if err != nil {
		t.Fatal(err)
	}
	if len(msgs) != 1 || msgs[0].ChatID != 42 || msgs[0].Text != "/watch moth.factory.testnet" {
		t.Fatalf("messages %+v", msgs)
	}
	if next != 12 {
		t.Fatalf("next offset %d, want 12", next)
	}
	if !strings.HasPrefix(requests[0], "/bottest-token/getUpdates ") {
		t.Fatalf("request %q", requests[0])
	}
}

func TestBotAPISendsTextToTheChat(t *testing.T) {
	var requests []string
	srv := botAPIServer(t, &requests)

	if err := telegram.NewBotAPI("test-token", srv.URL).Send(context.Background(), 42, "hello"); err != nil {
		t.Fatal(err)
	}
	var sent map[string]any
	_ = json.Unmarshal([]byte(strings.SplitN(requests[0], " ", 2)[1]), &sent)
	if sent["chat_id"] != float64(42) || sent["text"] != "hello" {
		t.Fatalf("sent %v", sent)
	}
}

func TestBotAPIErrorsNeverContainTheToken(t *testing.T) {
	api := telegram.NewBotAPI("secret-test-token", "http://127.0.0.1:1")
	err := api.Send(context.Background(), 1, "x")
	if err == nil || strings.Contains(err.Error(), "secret-test-token") {
		t.Fatalf("error %v", err)
	}
}
