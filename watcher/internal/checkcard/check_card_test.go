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
		loc  *time.Location
	}{
		{"normal_cap_full", utc(2026, 11, 13, 14, 24), time.UTC},
		{"early_open_cap_has_room", utc(2026, 11, 12, 13, 30), time.UTC},
		{"coin_without_new_pool_fields", utc(2026, 11, 13, 14, 24), time.UTC},
		{"bag_zero_large_holders", utc(2026, 11, 14, 3, 34), time.FixedZone("WAT", 3600)},
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
			got := checkcard.Render(views, tc.now, tc.loc)
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
	if !strings.Contains(got, "The creator group holds 0% of supply.\n") {
		t.Fatalf("got:\n%s", got)
	}
}

func TestCheckCardNamesItsTimeZone(t *testing.T) {
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
	lagos := time.FixedZone("WAT", 3600)
	got := checkcard.Render(views, utc(2026, 11, 13, 14, 24), lagos)
	for _, want := range []string{"Read at 15:24 WAT.", "until 16:02 WAT, in 38 min.", "refills at 14:02 WAT tomorrow"} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
	if utcCard := checkcard.Render(views, utc(2026, 11, 13, 14, 24), time.UTC); !strings.Contains(utcCard, "Read at 14:24 UTC.") {
		t.Errorf("UTC card:\n%s", utcCard)
	}
}

func TestEarlyWindowLineUsesTheRightWordForBlocks(t *testing.T) {
	for blocks, want := range map[uint64]string{
		1:  "Early supply cannot sell for 1 more block.\n",
		2:  "Early supply cannot sell for 2 more blocks.\n",
		40: "Early supply cannot sell for 40 more blocks.\n",
	} {
		views := eventsource.PreBuyViews{Name: "Moth Lamp", Symbol: "MOTH", BlocksLeft: blocks, SharedAllowance: "0", EarlySupplyTotal: "0", LaunchNs: "1794488520000000000", WindowEarlyS: 86400}
		if got := checkcard.Render(views, utc(2026, 11, 13, 14, 24), time.UTC); !strings.Contains(got, want) {
			t.Errorf("blocks_left %d: missing %q in:\n%s", blocks, want, got)
		}
	}
}
