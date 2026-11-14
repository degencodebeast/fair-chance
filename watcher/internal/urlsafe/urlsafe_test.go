package urlsafe_test

import (
	"bytes"
	"log"
	"strings"
	"testing"

	"fairchance/watcher/internal/urlsafe"
)

const fakeKey = "FAKEKEY0123456789"

func TestAnEndpointWithAUserQueryFragmentOrMainnetIsRefusedAndNotEchoed(t *testing.T) {
	for _, raw := range []string{
		"https://rpc.testnet.near.org/?key=" + fakeKey,
		"https://rpc.testnet.invalid/v3/" + fakeKey + "?k=1",
		"https://" + fakeKey + ":x@rpc.testnet.near.org",
		"https://rpc.testnet.near.org/#" + fakeKey,
		"https://rpc.mainnet.near.org",
		"ftp://rpc.testnet.near.org",
		"not a url " + fakeKey,
	} {
		err := urlsafe.CheckEndpoint("FC_RPC_URL", raw)
		if err == nil {
			t.Fatalf("accepted %q", raw)
		}
		if strings.Contains(err.Error(), fakeKey) || strings.Contains(err.Error(), "://") {
			t.Fatalf("the error echoes the URL: %v", err)
		}
	}
	for _, raw := range []string{
		"https://rpc.testnet.near.org",
		"https://rpc.testnet.near.org/",
		"https://example-testnet.invalid/abc123def/",
		"https://example-testnet.invalid/v1/abc123def",
		"https://tx.test.fastnear.com",
		"http://127.0.0.1:3030",
	} {
		if err := urlsafe.CheckEndpoint("FC_RPC_URL", raw); err != nil {
			t.Fatalf("refused %q: %v", raw, err)
		}
	}
}

func TestRedactReplacesEveryURL(t *testing.T) {
	in := `Post "https://api.telegram.org/bot` + fakeKey + `/sendMessage": dial tcp; then http://127.0.0.1:1/x failed`
	out := urlsafe.Redact(in)
	if strings.Contains(out, fakeKey) || strings.Contains(out, "127.0.0.1") || strings.Contains(out, "://") {
		t.Fatalf("a URL is left: %s", out)
	}
	if strings.Count(out, "<rpc url>") != 2 || !strings.HasPrefix(out, `Post "<rpc url>": dial tcp`) {
		t.Fatalf("unexpected text: %s", out)
	}
}

func TestTheLogWriterDropsURLs(t *testing.T) {
	var buf bytes.Buffer
	logger := log.New(urlsafe.Writer{W: &buf}, "", 0)
	logger.Printf("read pass: Post %q: EOF", "https://rpc.testnet.near.org/"+fakeKey)
	if strings.Contains(buf.String(), fakeKey) || !strings.Contains(buf.String(), "<rpc url>") {
		t.Fatalf("log line: %s", buf.String())
	}
}
