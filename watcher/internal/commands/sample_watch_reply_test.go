package commands_test

import (
	"context"
	"encoding/json"
	"os"
	"testing"

	"fairchance/watcher/internal/telegram"
)

func TestTheAlertsScreenSamplesTheWatchReplyTheBotSends(t *testing.T) {
	raw, err := os.ReadFile("../../../testdata/watcher-sample-messages/messages.json")
	if err != nil {
		t.Fatal(err)
	}
	var sample struct {
		Coin       struct{ Account string } `json:"coin"`
		WatchReply string                   `json:"watch_reply"`
	}
	if err := json.Unmarshal(raw, &sample); err != nil {
		t.Fatal(err)
	}
	bot, _, fake := newBot()
	bot.Handle(context.Background(), telegram.Incoming{ChatID: builderChat, Text: "/watch " + sample.Coin.Account})
	if len(fake.Sent) != 1 || fake.Sent[0].Text != sample.WatchReply {
		t.Fatalf("the bot sends %+v, the sample says %q", fake.Sent, sample.WatchReply)
	}
}
