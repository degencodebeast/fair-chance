// Package alerts turns one coin event into one plain sentence, from the event fields only.
package alerts

import (
	"encoding/json"
	"fmt"
	"math/big"
	"strconv"
	"strings"
	"time"

	"fairchance/watcher/internal/eventsource"
)

const (
	tokenDecimals = 18
	nearDecimals  = 24
	// Every coin mints 1,000,000,000 tokens once (the fixed values table).
	supplyWholeTokens = 1_000_000_000
)

// CoinRules is what the sentences need to know about one coin, read once from its views.
type CoinRules struct {
	Creator     string // fc_rules.creator
	LaunchBlock uint64 // fc_rules.launch_block
	EarlyBlocks uint64 // fc_rules.early_blocks
	Symbol      string // ft_metadata.symbol
}

// Writer turns events into sentences. The zero value works: it names tokens "tokens", shows
// times in UTC, and writes no creator-group or early-block sentence for a coin it has no rules for.
type Writer struct {
	Rules    func(coin string) (CoinRules, bool) // nil means no coin has rules
	Location *time.Location                      // for HH:MM times; nil means UTC
}

type sell struct {
	Account       string `json:"account"`
	Class         string `json:"class"`
	TokensAsked   string `json:"tokens_asked"`
	TokensSold    string `json:"tokens_sold"`
	NearOut       string `json:"near_out"`
	Fill          string `json:"fill"`
	LimitReason   string `json:"limit_reason"`
	AllowanceLeft string `json:"allowance_left"`
	WindowEndNs   string `json:"window_end_ns"`
}

// Sentence gives the alert text for an event, or false when the event has no alert.
func (w Writer) Sentence(ev eventsource.Event) (string, bool) {
	if ev.Standard != "fair_chance" {
		return "", false
	}
	rules, _ := w.rulesFor(ev.Coin)
	symbol := rules.Symbol
	if symbol == "" {
		symbol = "tokens"
	}
	switch ev.Name {
	case "fc_sell":
		return w.sellSentence(ev, symbol)
	case "fc_cap_filled":
		var e struct {
			CapUsed     string `json:"cap_used"`
			WindowEndNs string `json:"window_end_ns"`
		}
		if json.Unmarshal(ev.Data, &e) != nil {
			return "", false
		}
		return fmt.Sprintf("Pool cap full for this hour: %s NEAR out. Limited sells resume at %s.",
			Amount(e.CapUsed, nearDecimals, 2), w.clock(e.WindowEndNs)), true
	case "fc_became_large":
		var e struct {
			Account string `json:"account"`
			Balance string `json:"balance"`
		}
		if json.Unmarshal(ev.Data, &e) != nil {
			return "", false
		}
		return fmt.Sprintf("%s now holds %s of supply. It is large and cannot send tokens.", e.Account, SupplyShare(e.Balance)), true
	case "fc_buy":
		var e struct {
			Account      string `json:"account"`
			TokensOut    string `json:"tokens_out"`
			MarkedTokens string `json:"marked_tokens"`
		}
		if json.Unmarshal(ev.Data, &e) != nil || isZero(e.MarkedTokens) {
			return "", false
		}
		bought := Compact(e.TokensOut) + " " + symbol
		if inCreatorGroup(e.Account, rules.Creator) {
			return fmt.Sprintf("%s (creator group) bought %s. These tokens count as early supply.", e.Account, bought), true
		}
		if rules.Creator == "" || ev.Block < rules.LaunchBlock {
			return "", false
		}
		return fmt.Sprintf("%s bought %s in block %d. These tokens count as early supply.", e.Account, bought, ev.Block-rules.LaunchBlock), true
	case "fc_send":
		var e struct {
			To             string `json:"to"`
			Amount         string `json:"amount"`
			ToMarkedAmount string `json:"to_marked_amount"`
		}
		if json.Unmarshal(ev.Data, &e) != nil || isZero(e.ToMarkedAmount) || !inCreatorGroup(e.To, rules.Creator) {
			return "", false
		}
		return fmt.Sprintf("%s (creator group) received %s %s. These tokens count as early supply.", e.To, Compact(e.Amount), symbol), true
	}
	return "", false
}

func (w Writer) rulesFor(coin string) (CoinRules, bool) {
	if w.Rules == nil {
		return CoinRules{}, false
	}
	return w.Rules(coin)
}

func (w Writer) sellSentence(ev eventsource.Event, symbol string) (string, bool) {
	var s sell
	if err := json.Unmarshal(ev.Data, &s); err != nil {
		return "", false
	}
	if s.Fill == "full" && s.LimitReason == "" {
		return fmt.Sprintf("%s (%s holder) sold %s tokens for %s NEAR. The whole sell filled.",
			s.Account, s.Class, Amount(s.TokensSold, tokenDecimals, 2), Amount(s.NearOut, nearDecimals, 4)), true
	}
	text := fmt.Sprintf("%s tried to sell %s %s (%s of supply). The rule filled %s (%s).",
		s.Account, Compact(s.TokensAsked), symbol, SupplyShare(s.TokensAsked), Compact(s.TokensSold), SupplyShare(s.TokensSold))
	if !isZero(s.AllowanceLeft) {
		text += fmt.Sprintf(" It can sell up to %s more after %s.", Compact(s.AllowanceLeft), w.clock(s.WindowEndNs))
	} else {
		text += fmt.Sprintf(" Its allowance is used up until %s.", w.clock(s.WindowEndNs))
	}
	return text, true
}

func (w Writer) clock(ns string) string {
	n, err := strconv.ParseInt(ns, 10, 64)
	if err != nil {
		return ns
	}
	loc := w.Location
	if loc == nil {
		loc = time.UTC
	}
	return time.Unix(0, n).In(loc).Format("15:04")
}

// inCreatorGroup: the creator, every sub-account of the creator, and every parent of the creator
// below the top level. The test is on the dot boundary, so malice.near is not in the group of alice.near.
func inCreatorGroup(account, creator string) bool {
	if creator == "" || account == "" {
		return false
	}
	return account == creator ||
		strings.HasSuffix(account, "."+creator) ||
		(strings.Contains(account, ".") && strings.HasSuffix(creator, "."+account))
}

func isZero(base string) bool {
	n, ok := new(big.Int).SetString(base, 10)
	return !ok || n.Sign() == 0
}

// Compact writes a token amount (base units) as 7.1M, 100K or 12, cut and not rounded.
func Compact(base string) string {
	n, ok := new(big.Int).SetString(base, 10)
	if !ok || n.Sign() < 0 {
		return base
	}
	whole := new(big.Int).Div(n, pow10(tokenDecimals))
	switch {
	case whole.Cmp(big.NewInt(1_000_000)) >= 0:
		tenths := new(big.Int).Div(new(big.Int).Mul(n, big.NewInt(10)), pow10(tokenDecimals+6))
		return fmt.Sprintf("%d.%dM", new(big.Int).Div(tenths, big.NewInt(10)), new(big.Int).Mod(tenths, big.NewInt(10)))
	case whole.Cmp(big.NewInt(1_000)) >= 0:
		return new(big.Int).Div(whole, big.NewInt(1_000)).String() + "K"
	}
	return whole.String()
}

// SupplyShare writes an amount as a share of the fixed supply: 3.4%, 0.7%, 0.01% or <0.01%.
func SupplyShare(base string) string {
	n, ok := new(big.Int).SetString(base, 10)
	if !ok || n.Sign() <= 0 {
		return "0%"
	}
	supply := new(big.Int).Mul(big.NewInt(supplyWholeTokens), pow10(tokenDecimals))
	bps := new(big.Int).Div(new(big.Int).Mul(n, big.NewInt(10_000)), supply).Int64()
	if bps == 0 {
		return "<0.01%"
	}
	return FormatBps(bps)
}

// FormatBps writes basis points as a percent, cut and not rounded: 3.9%, 0.07% or 0%.
func FormatBps(bps int64) string {
	switch {
	case bps == 0:
		return "0%"
	case bps < 10:
		return fmt.Sprintf("0.%02d%%", bps)
	}
	return fmt.Sprintf("%d.%d%%", bps/100, bps%100/10)
}

func pow10(n int) *big.Int { return new(big.Int).Exp(big.NewInt(10), big.NewInt(int64(n)), nil) }

// Amount writes a base-unit decimal string in whole units, cut (not rounded) to `places`
// decimals, with commas between thousands. A value that does not parse is written as given.
func Amount(base string, decimals, places int) string {
	n, ok := new(big.Int).SetString(base, 10)
	if !ok || n.Sign() < 0 {
		return base
	}
	whole, frac := new(big.Int).QuoRem(n, pow10(decimals), new(big.Int))
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
