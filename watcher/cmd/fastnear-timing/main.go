// Command fastnear-timing measures, for each event of one coin, the time from the block that holds
// the event's receipt to the first FastNEAR read that shows the event. Start it before the run it
// measures; it waits for the coin to exist. It also gives the testnet seconds per block from the
// coin's launch block to the final head at the end.
//
// It reads only public endpoints with no key: FastNEAR (-fastnear) and NEAR RPC (-rpc). On a 429 it
// waits and reads again, at most twice and at most 15 minutes in all for the whole tool; past that,
// on a 403 or on an answer it does not know, it stops, writes what it has and exits 1. It signs
// nothing.
//
// The latency is local clock minus chain block time, so a skewed local clock moves every value by
// the same amount. The report says so.
package main

import (
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"log"
	"os"
	"strings"
	"time"

	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/timing"
	"fairchance/watcher/internal/urlsafe"
)

type report struct {
	Coin            string          `json:"coin"`
	FastNearURL     string          `json:"fastnear_url"`
	RPCURL          string          `json:"rpc_url"`
	PollSeconds     float64         `json:"poll_s"`
	LaunchBlock     uint64          `json:"launch_block"`
	Events          []row           `json:"events"`
	Summary         timing.Summary  `json:"latency_summary"`
	SecondsPerBlock *blocksMeasured `json:"seconds_per_block,omitempty"`
	RateLimited     int             `json:"rate_limited_reads"`
	RateLimitWaits  int             `json:"rate_limit_waits"` // waits obeyed; at most 2 in one run
	StoppedBy       string          `json:"stopped_by"`
	Note            string          `json:"note"`
}

type row struct {
	timing.Seen
	TxSigner       string  `json:"tx_signer"`
	LatencySeconds float64 `json:"latency_s"`
}

type blocksMeasured struct {
	FromHeight uint64  `json:"from_height"`
	ToHeight   uint64  `json:"to_height"`
	Seconds    float64 `json:"seconds_per_block"`
}

func main() {
	coin := flag.String("coin", "", "coin account, for example bag.factory.fairchance.testnet")
	rpcURL := flag.String("rpc", "https://rpc.testnet.near.org", "public NEAR testnet RPC")
	fastNearURL := flag.String("fastnear", "https://tx.test.fastnear.com", "public FastNEAR testnet transactions API")
	poll := flag.Duration("poll", time.Second, "time between FastNEAR reads")
	timeout := flag.Duration("timeout", 30*time.Minute, "longest run")
	idle := flag.Duration("idle", 3*time.Minute, "stop after this long with no new event, once one is seen")
	want := flag.Int("want", 0, "stop after this many events (0: no limit)")
	out := flag.String("out", "", "also write the report JSON to this file")
	flag.Parse()
	log.SetFlags(0)
	log.SetPrefix("fastnear-timing: ")
	log.SetOutput(urlsafe.Writer{W: os.Stderr})
	if *coin == "" {
		log.Fatal("set -coin")
	}
	if err := checkEndpoints(*rpcURL, *fastNearURL); err != nil {
		log.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), *timeout)
	defer cancel()

	r := report{Coin: *coin, FastNearURL: *fastNearURL, RPCURL: *rpcURL, PollSeconds: poll.Seconds(),
		Note: "latency_s = local time of the first FastNEAR read that showed the event minus the receipt block timestamp; local clock skew shifts every value"}
	// One 429 limit for the whole tool: the coin wait, the polling and finish share it.
	back := limitedBackOff(2 * time.Second)
	err := runLimited(ctx, &r, *poll, *idle, *want, defaultCoinWaitPoll, &back)
	if err != nil {
		r.StoppedBy = urlsafe.Redact(err.Error())
	}
	if ferr := finishLimited(ctx, &r, &back); ferr != nil && (err == nil || errors.Is(err, errDone)) {
		err = ferr
	}
	raw, _ := json.MarshalIndent(r, "", "  ")
	fmt.Println(string(raw))
	if *out != "" {
		if werr := os.WriteFile(*out, append(raw, '\n'), 0o644); werr != nil {
			log.Print(werr)
		}
	}
	if err != nil && !errors.Is(err, errDone) {
		os.Exit(1)
	}
}

var errDone = errors.New("done")

// defaultCoinWaitPoll is the time between RPC reads while the coin does not exist yet. The public
// RPC rate limits one address, and the run, the watcher and this tool share it.
const defaultCoinWaitPoll = 5 * time.Second

// run waits for the coin, then reads FastNEAR every poll and keeps the first-read time per event.
// checkEndpoints refuses an RPC or FastNEAR URL that could hold a key, without echoing it.
func checkEndpoints(rpcURL, fastNearURL string) error {
	if err := urlsafe.CheckEndpoint("-rpc", rpcURL); err != nil {
		return err
	}
	return urlsafe.CheckEndpoint("-fastnear", fastNearURL)
}

func run(ctx context.Context, r *report, poll, idle time.Duration, want int) error {
	return runWithCoinWait(ctx, r, poll, idle, want, defaultCoinWaitPoll)
}

func runWithCoinWait(ctx context.Context, r *report, poll, idle time.Duration, want int, coinPoll time.Duration) error {
	return runWithCoinWaitAndBackOff(ctx, r, poll, idle, want, coinPoll, 2*time.Second)
}

// The builder's 429 rule: obey Retry-After at most twice and wait at most 15 minutes in all, then
// stop and report. No other retry loop.
const (
	maxRateLimitWaits  = 2
	maxRateLimitWaited = 15 * time.Minute
)

func limitedBackOff(firstWait time.Duration) timing.LimitedBackOff {
	return timing.LimitedBackOff{
		BackOff:  timing.BackOff{Start: firstWait, Max: time.Minute},
		MaxWaits: maxRateLimitWaits,
		Total:    maxRateLimitWaited,
	}
}

func rateLimitStop(who string) error {
	return fmt.Errorf("429 from %s past the limit (%d waits, %s in all); stopped, no retry", who, maxRateLimitWaits, maxRateLimitWaited)
}

func runWithCoinWaitAndBackOff(ctx context.Context, r *report, poll, idle time.Duration, want int, coinPoll, firstWait time.Duration) error {
	back := limitedBackOff(firstWait)
	return runLimited(ctx, r, poll, idle, want, coinPoll, &back)
}

// runLimited is run with the caller's 429 limit, so later steps of the tool share it.
func runLimited(ctx context.Context, r *report, poll, idle time.Duration, want int, coinPoll time.Duration, back *timing.LimitedBackOff) error {
	var facts eventsource.CoinFacts
	for {
		f, err := eventsource.ReadCoinFacts(ctx, r.RPCURL, r.Coin)
		if err == nil {
			facts = f
			break
		}
		var limited *eventsource.RateLimitedError
		switch {
		case errors.As(err, &limited):
			r.RateLimited++
			wait, ok := back.Next(limited.RetryAfter)
			if !ok {
				return rateLimitStop("the RPC")
			}
			r.RateLimitWaits++
			log.Printf("429 from the RPC; waiting %s", wait)
			sleep(ctx, wait)
		case errors.Is(err, eventsource.ErrRateLimited):
			r.RateLimited++
			wait, ok := back.Next(0)
			if !ok {
				return rateLimitStop("the RPC")
			}
			r.RateLimitWaits++
			sleep(ctx, wait)
		case strings.Contains(err.Error(), "UNKNOWN_ACCOUNT"):
			sleep(ctx, coinPoll) // not launched yet
		default:
			return fmt.Errorf("reading %s: %w", r.Coin, err)
		}
		if ctx.Err() != nil {
			return fmt.Errorf("%s did not appear: %w", r.Coin, ctx.Err())
		}
	}
	back.Reset()
	r.LaunchBlock = facts.LaunchBlock
	start := eventsource.ScanStart(facts.LaunchBlock)
	log.Printf("%s launched at block %d; reading FastNEAR every %s", r.Coin, facts.LaunchBlock, poll)

	src := eventsource.NewFastNear(r.FastNearURL)
	seen := map[string]bool{}
	since, last := start, time.Now()
	failures := 0
	for {
		evs, next, err := src.Events(ctx, []string{r.Coin}, since)
		now := time.Now()
		var limited *eventsource.RateLimitedError
		switch {
		case err == nil:
			failures = 0
			back.Reset()
		case errors.As(err, &limited):
			r.RateLimited++
			wait, ok := back.Next(limited.RetryAfter)
			if !ok {
				return rateLimitStop("FastNEAR")
			}
			r.RateLimitWaits++
			log.Printf("429 from FastNEAR; waiting %s", wait)
			sleep(ctx, wait)
			continue
		case errors.Is(err, eventsource.ErrForbidden), errors.Is(err, eventsource.ErrUnexpectedResponse):
			return fmt.Errorf("stopped, no retry: %w", err)
		case ctx.Err() != nil:
			return fmt.Errorf("time limit: %w", ctx.Err())
		default:
			failures++
			if failures >= 5 {
				return fmt.Errorf("five reads in a row failed, last: %w", err)
			}
			log.Printf("read failed (%d in a row): %v", failures, err)
			sleep(ctx, poll)
			continue
		}
		// One receipt can log several events (fc_sell and fc_cap_filled): key each by its receipt
		// and its place among that receipt's events, which is the log order and the same each read.
		place := map[string]int{}
		for _, ev := range evs {
			key := fmt.Sprintf("%s#%d", ev.ReceiptID, place[ev.ReceiptID])
			place[ev.ReceiptID]++
			if seen[key] {
				continue
			}
			seen[key] = true
			last = now
			r.Events = append(r.Events, row{Seen: timing.Seen{Event: ev.Name, ReceiptID: ev.ReceiptID, TxHash: ev.TxHash,
				SeenAtNs: uint64(now.UnixNano())}, TxSigner: ev.TxSigner})
			log.Printf("first read of %s receipt %s (tx %s)", ev.Name, ev.ReceiptID, ev.TxHash)
		}
		// Read again from a few blocks back, so a receipt indexed after its transaction is not missed.
		if next > since+20 {
			since = next - 20
		}
		switch {
		case want > 0 && len(r.Events) >= want:
			return errDone
		case len(r.Events) > 0 && time.Since(last) > idle:
			return errDone
		case ctx.Err() != nil:
			return fmt.Errorf("time limit: %w", ctx.Err())
		}
		sleep(ctx, poll)
	}
}

// finish reads each receipt's block time and the seconds per block over RPC, with its own time
// limit so a stopped run still gets them. It waits and reads again only on a 429. Any other error
// (a 403, an answer it does not know, a timeout) stops it at once: StoppedBy says why, the events
// read so far stay in the report and the error comes back, so the tool exits 1.
func finish(parent context.Context, r *report) error {
	return finishWithBackOff(parent, r, 2*time.Second)
}

func finishWithBackOff(parent context.Context, r *report, firstWait time.Duration) error {
	back := limitedBackOff(firstWait)
	return finishLimited(parent, r, &back)
}

// finishLimited is finish with the caller's 429 limit, the one the run already used.
func finishLimited(parent context.Context, r *report, back *timing.LimitedBackOff) error {
	ctx, cancel := context.WithTimeout(context.WithoutCancel(parent), 5*time.Minute)
	defer cancel()
	retry := func(read func() error) error {
		for {
			err := read()
			if !errors.Is(err, eventsource.ErrRateLimited) || ctx.Err() != nil {
				return err
			}
			r.RateLimited++
			var limited *eventsource.RateLimitedError
			var after time.Duration
			if errors.As(err, &limited) {
				after = limited.RetryAfter
			}
			wait, ok := back.Next(after)
			if !ok {
				return fmt.Errorf("%w; %w", err, rateLimitStop("the RPC"))
			}
			r.RateLimitWaits++
			sleep(ctx, wait)
		}
	}
	stop := func(what string, err error) error {
		err = fmt.Errorf("%s: %w; stopped, no retry", what, err)
		if r.StoppedBy == "" {
			r.StoppedBy = urlsafe.Redact(err.Error())
		} else {
			r.StoppedBy += "; then " + urlsafe.Redact(err.Error())
		}
		return err
	}
	done := 0
	times := map[string]eventsource.BlockTime{} // per receipt: one receipt can hold several events
	for i := range r.Events {
		e := &r.Events[i]
		bt, known := times[e.ReceiptID]
		if !known {
			err := retry(func() (err error) {
				bt, err = eventsource.ReceiptBlockTime(ctx, r.RPCURL, e.TxHash, e.TxSigner, e.ReceiptID)
				return err
			})
			if err != nil {
				r.Events = r.Events[:done]
				r.Summary = summarize(r.Events)
				return stop("receipt "+e.ReceiptID, err)
			}
			times[e.ReceiptID] = bt
		}
		e.ReceiptBlock, e.ReceiptBlockNs = bt.Height, bt.Ns
		e.LatencySeconds = e.Seen.LatencySeconds()
		done++
	}
	r.Summary = summarize(r.Events)
	if r.LaunchBlock == 0 {
		return nil
	}
	var from, to eventsource.BlockTime
	if err := retry(func() (err error) { from, err = eventsource.BlockTimeAt(ctx, r.RPCURL, r.LaunchBlock); return err }); err != nil {
		return stop("launch block", err)
	}
	if err := retry(func() (err error) { to, err = eventsource.BlockTimeAt(ctx, r.RPCURL, 0); return err }); err != nil {
		return stop("final head", err)
	}
	spb, err := timing.SecondsPerBlock(from.Height, from.Ns, to.Height, to.Ns)
	if err != nil {
		return stop("seconds per block", err)
	}
	r.SecondsPerBlock = &blocksMeasured{FromHeight: from.Height, ToHeight: to.Height, Seconds: spb}
	return nil
}

func summarize(rows []row) timing.Summary {
	seen := make([]timing.Seen, len(rows))
	for i, e := range rows {
		seen[i] = e.Seen
	}
	return timing.Summarize(seen)
}

func sleep(ctx context.Context, d time.Duration) {
	select {
	case <-ctx.Done():
	case <-time.After(d):
	}
}
