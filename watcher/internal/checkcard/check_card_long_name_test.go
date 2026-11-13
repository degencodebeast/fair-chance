package checkcard_test

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
	"unicode/utf16"

	"fairchance/watcher/internal/checkcard"
	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
)

func TestALongCoinNameAndSymbolNeverPushTheRuleFactsOffTheCard(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join("testdata", "normal_cap_full.views.json"))
	if err != nil {
		t.Fatal(err)
	}
	var byMethod map[string]json.RawMessage
	if err := json.Unmarshal(raw, &byMethod); err != nil {
		t.Fatal(err)
	}
	views, err := eventsource.ParsePreBuyViews(byMethod)
	if err != nil {
		t.Fatal(err)
	}
	views.Name = strings.Repeat("N", 4100)
	views.Symbol = strings.Repeat("S", 4100)

	card := checkcard.Render(views, utc(2026, 11, 13, 14, 24), time.UTC)
	sent := telegram.Prepare(card)
	for _, want := range []string{"Pool cap this hour", "Nobody can change these rules."} {
		if !strings.Contains(sent, want) {
			t.Fatalf("the sent card lost %q", want)
		}
	}
	if n := len(utf16.Encode([]rune(card))); n > telegram.MaxMessageUnits {
		t.Fatalf("the card is %d units before the send", n)
	}
	if !strings.Contains(card, "…") {
		t.Fatal("a cut name must end with an ellipsis")
	}
}
