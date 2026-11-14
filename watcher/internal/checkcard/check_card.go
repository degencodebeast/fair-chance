// Package checkcard writes the /check reply: the "who can sell" facts of one coin as plain text.
package checkcard

import (
	"context"
	"fmt"
	"math/big"
	"strconv"
	"strings"
	"time"

	"fairchance/watcher/internal/alerts"
	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
)

const (
	nearDecimals = 24
	notAvailable = "not available from this coin"
)

// Render writes the card. Times are in loc, each with a hint of how long from now.
func Render(v eventsource.PreBuyViews, now time.Time, loc *time.Location) string {
	symbol := telegram.Label(v.Symbol)
	if symbol == "" {
		symbol = "tokens"
	}
	var b strings.Builder
	fmt.Fprintf(&b, "%s (%s)\nRead at %s.\n\n", telegram.Label(v.Name), telegram.Label(v.Symbol), now.In(loc).Format("15:04 MST"))

	if v.CreatorGroupShareBps == nil {
		fmt.Fprintf(&b, "The creator group's share of supply is %s.\n", notAvailable)
	} else {
		fmt.Fprintf(&b, "The creator group holds %s of supply.\n", alerts.FormatBps(int64(*v.CreatorGroupShareBps)))
	}
	fmt.Fprintf(&b, "Early supply holds %s of supply.\n", alerts.SupplyShare(v.EarlySupplyTotal))
	if v.BlocksLeft > 0 {
		fmt.Fprintf(&b, "Early supply cannot sell for %d more blocks.\n", v.BlocksLeft)
	} else {
		fmt.Fprintf(&b, "Early supply can sell %s %s in total in this 24 h window. The limit refills at %s.\n", alerts.Compact(v.SharedAllowance), symbol, whenText(nextRefill(v, now), now, loc))
	}

	b.WriteString("\n")
	if v.LargeHolders == nil {
		fmt.Fprintf(&b, "Large holders: %s\n", notAvailable)
	} else {
		fmt.Fprintf(&b, "Large holders: %d. A large holder cannot send tokens to another account.\n", *v.LargeHolders)
	}

	b.WriteString("\n")
	reset := whenText(time.Unix(0, parseNs(v.CapWindowEndNs)), now, loc)
	if v.CapIsFull {
		fmt.Fprintf(&b, "Pool cap this hour: full, %s NEAR of %s NEAR out.\n", near(v.CapUsed), near(v.CapTotal))
		fmt.Fprintf(&b, "Large and early-supply sells fill a small floor until %s.\n", reset)
	} else {
		fmt.Fprintf(&b, "Pool cap this hour: %s NEAR left of %s NEAR. It resets at %s.\n", near(v.CapLeft), near(v.CapTotal), reset)
	}

	b.WriteString("\nNobody can change these rules, not even the creator.")
	return b.String()
}

// nextRefill is the next start of a 24 h early-supply window on the grid from the launch time.
func nextRefill(v eventsource.PreBuyViews, now time.Time) time.Time {
	launch := time.Unix(0, parseNs(v.LaunchNs))
	length := time.Duration(v.WindowEarlyS) * time.Second
	if length <= 0 || now.Before(launch) {
		return launch
	}
	elapsedWindows := now.Sub(launch) / length
	return launch.Add((elapsedWindows + 1) * length)
}

func parseNs(ns string) int64 {
	n, _ := strconv.ParseInt(ns, 10, 64)
	return n
}

func near(yocto string) string {
	if n, ok := new(big.Int).SetString(yocto, 10); !ok || n.Sign() < 0 {
		return yocto
	}
	return alerts.Amount(yocto, nearDecimals, 2)
}

// whenText writes a time as "15:02", "13:02 tomorrow" or "13:02 on 15 Nov", then ", in 38 min".
func whenText(at, now time.Time, loc *time.Location) string {
	at, now = at.In(loc), now.In(loc)
	text := at.Format("15:04 MST")
	days := daysBetween(now, at)
	switch {
	case days == 1:
		text += " tomorrow"
	case days > 1:
		text += " on " + at.Format("2 Jan")
	}
	return text + ", " + relative(at.Sub(now))
}

func daysBetween(from, to time.Time) int {
	f := time.Date(from.Year(), from.Month(), from.Day(), 0, 0, 0, 0, time.UTC)
	t := time.Date(to.Year(), to.Month(), to.Day(), 0, 0, 0, 0, time.UTC)
	return int(t.Sub(f).Hours() / 24)
}

// relative rounds up to whole minutes: "in 38 min", "in 22 h 38 min".
func relative(d time.Duration) string {
	minutes := int((d + time.Minute - 1) / time.Minute)
	switch {
	case minutes <= 0:
		return "now"
	case minutes < 60:
		return fmt.Sprintf("in %d min", minutes)
	case minutes%60 == 0:
		return fmt.Sprintf("in %d h", minutes/60)
	}
	return fmt.Sprintf("in %d h %d min", minutes/60, minutes%60)
}

// Checker reads the views of a coin over RPC (view calls only, nothing is signed) and writes the card.
type Checker struct {
	RPCURL   string
	Now      func() time.Time // nil means the clock
	Location *time.Location   // nil means UTC
}

func (c Checker) Check(ctx context.Context, coin string) (string, error) {
	views, err := eventsource.ReadPreBuyViews(ctx, c.RPCURL, coin)
	if err != nil {
		return "", err
	}
	now, loc := time.Now(), c.Location
	if c.Now != nil {
		now = c.Now()
	}
	if loc == nil {
		loc = time.UTC
	}
	return Render(views, now, loc), nil
}
