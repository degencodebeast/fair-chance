package alerts_test

import (
	"encoding/json"
	"os"
	"testing"
	"time"

	"fairchance/watcher/internal/alerts"
	"fairchance/watcher/internal/eventsource"
)

// The alerts screen of the web page shows these messages. Each one is the sentence the watcher's
// own templates write for the event beside it, so the page cannot drift from the bot.
type sampleMessages struct {
	Coin struct {
		Account, Creator string
		LaunchBlock      uint64 `json:"launch_block"`
		EarlyBlocks      uint64 `json:"early_blocks"`
		Symbol           string
	} `json:"coin"`
	Alerts []struct {
		Label string `json:"label"`
		Event struct {
			Name   string         `json:"name"`
			Block  uint64         `json:"block"`
			Fields map[string]any `json:"fields"`
		} `json:"event"`
		Text string `json:"text"`
	} `json:"alerts"`
}

func TestTheAlertsScreenSamplesAreTheWatcherTemplatesFilledWithExampleValues(t *testing.T) {
	raw, err := os.ReadFile("../../../testdata/watcher-sample-messages/messages.json")
	if err != nil {
		t.Fatal(err)
	}
	var sample sampleMessages
	if err := json.Unmarshal(raw, &sample); err != nil {
		t.Fatal(err)
	}
	if len(sample.Alerts) < 6 {
		t.Fatalf("the screen needs at least 6 sample alerts, the file has %d", len(sample.Alerts))
	}
	writer := alerts.Writer{
		Location: time.UTC,
		Rules: func(coin string) (alerts.CoinRules, bool) {
			return alerts.CoinRules{Creator: sample.Coin.Creator, LaunchBlock: sample.Coin.LaunchBlock, EarlyBlocks: sample.Coin.EarlyBlocks, Symbol: sample.Coin.Symbol}, true
		},
	}
	for _, a := range sample.Alerts {
		data, err := json.Marshal(a.Event.Fields)
		if err != nil {
			t.Fatal(err)
		}
		ev := eventsource.Event{Standard: "fair_chance", Name: a.Event.Name, Coin: sample.Coin.Account, Block: a.Event.Block, Data: data}
		got, ok := writer.Sentence(ev)
		if !ok || got != a.Text {
			t.Errorf("%s: the template writes %q (alert %v), the sample says %q", a.Label, got, ok, a.Text)
		}
	}
}
