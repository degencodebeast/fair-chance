// Command fastnear-timing measures, for each event of one coin, the time from the block that holds
// the event's receipt to the first FastNEAR read that shows the event. Start it before the run it
// measures; it waits for the coin to exist. It also gives the testnet seconds per block from the
// coin's launch block to the final head at the end.
//
// It reads only public endpoints with no key: FastNEAR (-fastnear) and NEAR RPC (-rpc). On a 429 it
// backs off and goes on; on a 403 or an answer it does not know it stops, writes what it has and
// exits 1. It signs nothing.
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
	if *coin == "" {
		log.Fatal("set -coin")
	}
	for _, u := range []string{*rpcURL, *fastNearURL} {
		if strings.Contains(u, "mainnet") || strings.ContainsAny(u, "?@") {
			log.Fatalf("use a public testnet endpoint with no key in the URL, not %s", u)
		}
	}
	ctx, cancel := context.WithTimeout(context.Background(), *timeout)
	defer cancel()

	r := report{Coin: *coin, FastNearURL: *fastNearURL, RPCURL: *rpcURL, PollSeconds: poll.Seconds(),
		Note: "latency_s = local time of the first FastNEAR read that showed the event minus the receipt block timestamp; local clock skew shifts every value"}
	err := run(ctx, &r, *poll, *idle, *want)
	if err != nil {
		r.StoppedBy = err.Error()
	}
	finish(ctx, &r)
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

// run waits for the coin, then reads FastNEAR every poll and keeps the first-read time per receipt.
func run(ctx context.Context, r *report, poll, idle time.Duration, want int) error {
	back := timing.BackOff{Start: 2 * time.Second, Max: time.Minute}
	var facts eventsource.CoinFacts
	for {
		f, err := eventsource.ReadCoinFacts(ctx, r.RPCURL, r.Coin)
		if err == nil {
			facts = f
			break
		}
		switch {
		case errors.Is(err, eventsource.ErrRateLimited):
			r.RateLimited++
			sleep(ctx, back.Next(0))
		case strings.Contains(err.Error(), "UNKNOWN_ACCOUNT"):
			sleep(ctx, poll) // not launched yet
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
			wait := back.Next(limited.RetryAfter)
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
		for _, ev := range evs {
			if seen[ev.ReceiptID] {
				continue
			}
			seen[ev.ReceiptID] = true
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
// limit so a stopped run still gets them.
func finish(parent context.Context, r *report) {
	ctx, cancel := context.WithTimeout(context.WithoutCancel(parent), 5*time.Minute)
	defer cancel()
	back := timing.BackOff{Start: 2 * time.Second, Max: time.Minute}
	retry := func(read func() error) error {
		for {
			err := read()
			if !errors.Is(err, eventsource.ErrRateLimited) || ctx.Err() != nil {
				return err
			}
			r.RateLimited++
			sleep(ctx, back.Next(0))
		}
	}
	kept := r.Events[:0]
	var seen []timing.Seen
	for _, e := range r.Events {
		var bt eventsource.BlockTime
		err := retry(func() (err error) {
			bt, err = eventsource.ReceiptBlockTime(ctx, r.RPCURL, e.TxHash, e.TxSigner, e.ReceiptID)
			return err
		})
		if err != nil {
			log.Printf("receipt %s: %v; left out of the report", e.ReceiptID, err)
			continue
		}
		e.ReceiptBlock, e.ReceiptBlockNs = bt.Height, bt.Ns
		e.LatencySeconds = e.Seen.LatencySeconds()
		kept = append(kept, e)
		seen = append(seen, e.Seen)
	}
	r.Events = kept
	r.Summary = timing.Summarize(seen)
	if r.LaunchBlock == 0 {
		return
	}
	var from, to eventsource.BlockTime
	if err := retry(func() (err error) { from, err = eventsource.BlockTimeAt(ctx, r.RPCURL, r.LaunchBlock); return err }); err != nil {
		log.Printf("launch block: %v", err)
		return
	}
	if err := retry(func() (err error) { to, err = eventsource.BlockTimeAt(ctx, r.RPCURL, 0); return err }); err != nil {
		log.Printf("final head: %v", err)
		return
	}
	if spb, err := timing.SecondsPerBlock(from.Height, from.Ns, to.Height, to.Ns); err == nil {
		r.SecondsPerBlock = &blocksMeasured{FromHeight: from.Height, ToHeight: to.Height, Seconds: spb}
	}
}

func sleep(ctx context.Context, d time.Duration) {
	select {
	case <-ctx.Done():
	case <-time.After(d):
	}
}
