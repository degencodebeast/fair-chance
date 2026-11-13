package telegram_test

import (
	"context"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"unicode/utf16"

	"fairchance/watcher/internal/telegram"
)

func TestCleanChainTextRemovesControlAndDirectionCharacters(t *testing.T) {
	cases := map[string]string{
		"a‮b‏c⁦d⁧e⁨f⁩g‪h‬i‎j؜k":                     "abcdefghijk",
		"a\x00b\x07c\x1bd\x7fe\u0085f\u009Fg\nh\ti": "abcdefg\nhi",
		"Moth Lamp, café 日本 👩‍💻 100%":               "Moth Lamp, café 日本 👩‍💻 100%",
	}
	for in, want := range cases {
		if got := telegram.CleanChainText(in); got != want {
			t.Errorf("CleanChainText(%q) = %q, want %q", in, got, want)
		}
	}
}

func units(s string) int { return len(utf16.Encode([]rune(s))) }

func TestComposeCutsTheDescriptionSoTheWholeMessageFits(t *testing.T) {
	head, tail := "Moth Lamp (MOTH)\n", "\nNobody can change these rules."
	for _, desc := range []string{strings.Repeat("x", 9000), strings.Repeat("😀", 3000)} {
		got := telegram.Compose(head, desc, tail)
		if units(got) > 4096 {
			t.Fatalf("message is %d UTF-16 units, want at most 4096", units(got))
		}
		if !strings.HasPrefix(got, head) || !strings.HasSuffix(got, tail) {
			t.Fatalf("head or tail was cut: %q...%q", got[:20], got[len(got)-20:])
		}
		if !strings.Contains(got, "…") {
			t.Fatal("a cut description must end with an ellipsis")
		}
	}
}

func TestComposeKeepsAShortDescriptionAndCleansIt(t *testing.T) {
	got := telegram.Compose("H\n", "a lamp‮ for moths", "\nT")
	if got != "H\na lamp for moths\nT" {
		t.Fatalf("got %q", got)
	}
}

func TestSendCleansAndFitsTheText(t *testing.T) {
	var body string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		body = string(raw)
		w.Write([]byte(`{"ok":true,"result":{}}`))
	}))
	defer srv.Close()
	api := telegram.NewBotAPI("test-token", srv.URL)
	if err := api.Send(context.Background(), 1, "bob‮.testnet sold "+strings.Repeat("y", 5000)); err != nil {
		t.Fatal(err)
	}
	if strings.Contains(body, `‮`) || strings.Contains(body, "‮") {
		t.Fatalf("the sent text still holds a direction character: %s", body[:80])
	}
	if len(body) > 4300 {
		t.Fatalf("the sent text was not cut: %d bytes", len(body))
	}
}

func TestFakeKeepsTheCleanedText(t *testing.T) {
	f := &telegram.Fake{}
	_ = f.Send(context.Background(), 1, "a‮b")
	if f.Sent[0].Text != "ab" {
		t.Fatalf("got %q", f.Sent[0].Text)
	}
}
