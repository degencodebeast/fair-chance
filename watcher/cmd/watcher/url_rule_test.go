package main

import (
	"strings"
	"testing"
)

const fakeKey = "FAKEKEY0123456789"

// The watcher refuses an RPC or FastNEAR URL with a user, query or fragment before it touches Telegram,
// and does not echo it.
func TestTheWatcherRefusesAURLWithAQueryAndDoesNotEchoIt(t *testing.T) {
	for _, env := range []string{"FC_RPC_URL", "FC_FASTNEAR_URL"} {
		t.Setenv("FC_RPC_URL", "https://rpc.testnet.near.org")
		t.Setenv("FC_FASTNEAR_URL", "https://tx.test.fastnear.com")
		t.Setenv("FC_EVENT_SOURCE", "fastnear")
		t.Setenv("FC_TELEGRAM_CHAT_ID", "1")
		t.Setenv("TELEGRAM_BOT_TOKEN", "")
		t.Setenv(env, "https://rpc.testnet.invalid/v3?key="+fakeKey)
		err := runTelegram(0)
		if err == nil || !strings.Contains(err.Error(), env+" is refused") || strings.Contains(err.Error(), fakeKey) {
			t.Fatalf("%s: %v", env, err)
		}
	}
}

// A keyed path URL passes the start-up checks. The run then stops at the missing bot token.
func TestTheWatcherAcceptsAKeyedPathURL(t *testing.T) {
	for _, env := range []string{"FC_RPC_URL", "FC_FASTNEAR_URL"} {
		t.Setenv("FC_RPC_URL", "https://rpc.testnet.near.org")
		t.Setenv("FC_FASTNEAR_URL", "https://tx.test.fastnear.com")
		t.Setenv("FC_EVENT_SOURCE", "fastnear")
		t.Setenv("FC_TELEGRAM_CHAT_ID", "1")
		t.Setenv("TELEGRAM_BOT_TOKEN", "")
		t.Setenv(env, "https://example-testnet.invalid/abc123def/")
		err := runTelegram(0)
		if err != nil && strings.Contains(err.Error(), "is refused") {
			t.Fatalf("%s: refused: %v", env, err)
		}
		if err != nil && strings.Contains(err.Error(), "abc123def") {
			t.Fatalf("%s: echoes the key: %v", env, err)
		}
	}
}
