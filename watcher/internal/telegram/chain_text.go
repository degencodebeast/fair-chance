package telegram

import (
	"strings"
	"unicode"
	"unicode/utf16"
)

// MaxMessageUnits is the longest message Telegram takes. It counts characters after parsing; the
// code counts UTF-16 units, which is never fewer, so a message that fits here fits there.
const MaxMessageUnits = 4096

const ellipsis = "…"

// CleanChainText removes Unicode control characters (C0, DEL and C1, except a newline) and the
// direction characters that can reorder or hide text, from text the chain gave: names, tickers,
// descriptions and account IDs.
func CleanChainText(s string) string {
	return strings.Map(func(r rune) rune {
		switch {
		case r == '\n':
			return r
		case unicode.IsControl(r):
			return -1
		case r == 0x061C, r == 0x200E, r == 0x200F, r >= 0x202A && r <= 0x202E, r >= 0x2066 && r <= 0x2069:
			return -1
		}
		return r
	}, s)
}

func units(s string) int {
	n := 0
	for _, r := range s {
		n += len(utf16.Encode([]rune{r}))
	}
	return n
}

// cutUnits keeps the start of s that fits in max UTF-16 units, never splitting a character.
func cutUnits(s string, max int) string {
	n := 0
	for i, r := range s {
		w := len(utf16.Encode([]rune{r}))
		if n+w > max {
			return s[:i]
		}
		n += w
	}
	return s
}

// Prepare cleans a message and cuts it to the Telegram limit. Every send goes through it.
func Prepare(text string) string {
	text = CleanChainText(text)
	if units(text) <= MaxMessageUnits {
		return text
	}
	return cutUnits(text, MaxMessageUnits-units(ellipsis)) + ellipsis
}

// Compose joins a fixed head, the creator's description and a fixed tail. It cleans the
// description and cuts only the description, with an ellipsis, so the whole message fits.
func Compose(head, description, tail string) string {
	description = CleanChainText(description)
	room := MaxMessageUnits - units(head) - units(tail)
	if units(description) > room {
		room -= units(ellipsis)
		if room < 0 {
			room = 0
		}
		description = cutUnits(description, room) + ellipsis
	}
	return Prepare(head + description + tail)
}

// MaxLabelRunes is the longest coin name or symbol a message shows.
const MaxLabelRunes = 64

// Label cleans a coin name or symbol from the chain and cuts it to MaxLabelRunes with an ellipsis,
// so a very long name cannot push the rest of a message past the Telegram limit.
func Label(s string) string {
	s = CleanChainText(s)
	r := []rune(s)
	if len(r) <= MaxLabelRunes {
		return s
	}
	return string(r[:MaxLabelRunes-1]) + ellipsis
}
