package main

import (
	"strings"
	"testing"
)

// The watcher refuses an RPC or FastNEAR URL with a path before it touches Telegram, and does not
// echo it.
func TestTheWatcherRefusesAURLWithAPath(t *testing.T) {
	const key = "FAKEKEY0123456789"
	for _, env := range []string{"FC_RPC_URL", "FC_FASTNEAR_URL"} {
		t.Setenv("FC_RPC_URL", "https://rpc.testnet.near.org")
		t.Setenv("FC_FASTNEAR_URL", "https://tx.test.fastnear.com")
		t.Setenv("FC_EVENT_SOURCE", "fastnear")
		t.Setenv("FC_TELEGRAM_CHAT_ID", "1")
		t.Setenv("TELEGRAM_BOT_TOKEN", "")
		t.Setenv(env, "https://rpc.testnet.invalid/v3/"+key)
		err := runTelegram(0)
		if err == nil || !strings.Contains(err.Error(), env) || strings.Contains(err.Error(), key) {
			t.Fatalf("%s: %v", env, err)
		}
	}
}
