package checkcard_test

import (
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"time"

	"fairchance/watcher/internal/checkcard"
	"fairchance/watcher/internal/eventsource"
)

func utc(y int, m time.Month, d, h, min int) time.Time {
	return time.Date(y, m, d, h, min, 0, 0, time.UTC)
}

// The views and the expected text are hand-written (see testdata/README.md).
func TestCheckCardFromFixedViews(t *testing.T) {
	cases := []struct {
		name string
		now  time.Time
	}{
		{"normal_cap_full", utc(2026, 11, 13, 14, 24)},
		{"early_open_cap_has_room", utc(2026, 11, 12, 13, 30)},
		{"coin_without_new_pool_fields", utc(2026, 11, 13, 14, 24)},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			raw, err := os.ReadFile(filepath.Join("testdata", tc.name+".views.json"))
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
			want, err := os.ReadFile(filepath.Join("testdata", tc.name+".golden.txt"))
			if err != nil {
				t.Fatal(err)
			}
			got := checkcard.Render(views, tc.now, time.UTC)
			if got != strings.TrimSuffix(string(want), "\n") {
				t.Fatalf("got:\n%s\nwant:\n%s", got, want)
			}
			assertCopyRules(t, got)
		})
	}
}

var bannedWords = regexp.MustCompile(`(?i)\b(safe|rug-proof|audited|guaranteed|first|only|AI|queued|pending|waits)\b`)

func assertCopyRules(t *testing.T, text string) {
	t.Helper()
	if m := bannedWords.FindString(text); m != "" {
		t.Errorf("banned word %q in %q", m, text)
	}
	for _, r := range text {
		if r > 127 {
			t.Errorf("non-ASCII character %q (em dash or emoji) in %q", r, text)
			break
		}
	}
}

func TestCreatorGroupWithNoTokensIsWrittenAsZeroPercent(t *testing.T) {
	zero := uint64(0)
	views := eventsource.PreBuyViews{Name: "Moth Lamp", Symbol: "MOTH", CreatorGroupShareBps: &zero, SharedAllowance: "0", EarlySupplyTotal: "0", LaunchNs: "1794488520000000000", WindowEarlyS: 86400}
	got := checkcard.Render(views, utc(2026, 11, 13, 14, 24), time.UTC)
	if !strings.Contains(got, "Creator group: 0% of supply\n") {
		t.Fatalf("got:\n%s", got)
	}
}
