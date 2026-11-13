package commands_test

import (
	"context"
	"strings"
	"testing"

	"fairchance/watcher/internal/checkcard"
	"fairchance/watcher/internal/commands"
	"fairchance/watcher/internal/telegram"
)

// A fake keyed URL on a refused port: the real Checker fails with a network error.
func TestCheckReplyHoldsNoURLWhenTheRPCFails(t *testing.T) {
	checker := checkcard.Checker{RPCURL: "https://127.0.0.1:1/SECRET-TOKEN/"}
	for _, chat := range []int64{builderChat, strangerChat} {
		fake := &telegram.Fake{}
		public, err := commands.NewPublicCheck(factory, "", nil)
		if err != nil {
			t.Fatal(err)
		}
		bot := commands.Bot{ChatID: builderChat, Reply: fake, Watcher: &recordingWatcher{}, Checker: checker, Public: public}
		bot.Handle(context.Background(), telegram.Incoming{ChatID: chat, Text: "/check " + demoCoin})
		if len(fake.Sent) != 1 {
			t.Fatalf("chat %d: sent %+v", chat, fake.Sent)
		}
		for _, s := range fake.Sent {
			if strings.Contains(s.Text, "SECRET-TOKEN") || strings.Contains(s.Text, "127.0.0.1") || strings.Contains(s.Text, "://") {
				t.Fatalf("reply holds a URL: %q", s.Text)
			}
		}
	}
}
