// Package urlsafe keeps keys that can hide in a URL out of the output. An endpoint URL may have a path
// (a provider key can sit there) but no user, password, query or fragment, and every URL in log or
// error text is replaced with "<rpc url>". A Telegram request URL holds the bot token, so this also
// covers net/http errors of the Telegram client.
package urlsafe

import (
	"fmt"
	"io"
	"net/url"
	"regexp"
	"strings"
)

// CheckEndpoint refuses an endpoint URL that is not http or https with no user, password, query or fragment, or that names
// mainnet. The error names the setting, never the URL.
func CheckEndpoint(name, raw string) error {
	u, err := url.Parse(raw)
	if err != nil ||
		(u.Scheme != "https" && u.Scheme != "http") ||
		u.Host == "" ||
		u.User != nil ||
		u.RawQuery != "" || u.ForceQuery ||
		u.Fragment != "" || strings.Contains(raw, "#") ||
		strings.Contains(u.Host, "mainnet") {
		return fmt.Errorf("%s is refused: use a testnet or local endpoint with no user, password, query or fragment", name)
	}
	return nil
}

var anyURL = regexp.MustCompile("(?i)https?://[^\\s\"'`<>)\\]},;]*")

// Redact replaces every http:// or https:// URL in text with "<rpc url>".
func Redact(text string) string {
	return anyURL.ReplaceAllString(text, "<rpc url>")
}

// Writer redacts each write before it reaches W. Use it as the log output.
type Writer struct{ W io.Writer }

func (w Writer) Write(p []byte) (int, error) {
	if _, err := w.W.Write([]byte(Redact(string(p)))); err != nil {
		return 0, err
	}
	return len(p), nil
}
