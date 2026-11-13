// Package alert turns one coin event into one plain sentence, from the event fields only.
package alert

import (
	"encoding/json"
	"fmt"
	"math/big"
	"strings"

	"fairchance/watcher/internal/events"
)

const (
	tokenDecimals = 18
	nearDecimals  = 24
)

type sell struct {
	Account     string `json:"account"`
	Class       string `json:"class"`
	TokensAsked string `json:"tokens_asked"`
	TokensSold  string `json:"tokens_sold"`
	NearOut     string `json:"near_out"`
	Fill        string `json:"fill"`
}

// Sentence gives the alert text for an event, or false when the event has no alert.
func Sentence(ev events.Event) (string, bool) {
	if ev.Standard != "fair_chance" || ev.Name != "fc_sell" {
		return "", false
	}
	var s sell
	if err := json.Unmarshal(ev.Data, &s); err != nil {
		return "", false
	}
	text := fmt.Sprintf("%s (%s holder) sold %s tokens for %s NEAR.",
		s.Account, s.Class, Amount(s.TokensSold, tokenDecimals, 2), Amount(s.NearOut, nearDecimals, 4))
	switch s.Fill {
	case "full":
		text += " The whole sell filled."
	default:
		text += fmt.Sprintf(" It asked to sell %s tokens.", Amount(s.TokensAsked, tokenDecimals, 2))
	}
	return text, true
}

// Amount writes a base-unit decimal string in whole units, cut (not rounded) to `places`
// decimals, with commas between thousands. A value that does not parse is written as given.
func Amount(base string, decimals, places int) string {
	n, ok := new(big.Int).SetString(base, 10)
	if !ok || n.Sign() < 0 {
		return base
	}
	unit := new(big.Int).Exp(big.NewInt(10), big.NewInt(int64(decimals)), nil)
	whole, frac := new(big.Int).QuoRem(n, unit, new(big.Int))
	fracText := fmt.Sprintf("%0*s", decimals, frac.String())[:places]
	return commas(whole.String()) + "." + fracText
}

func commas(digits string) string {
	var b strings.Builder
	for i, d := range digits {
		if i > 0 && (len(digits)-i)%3 == 0 {
			b.WriteByte(',')
		}
		b.WriteRune(d)
	}
	return b.String()
}
