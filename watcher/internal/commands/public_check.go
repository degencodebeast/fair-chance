package commands

import (
	"context"
	"errors"
	"fmt"
	"regexp"
	"strings"
	"sync"
	"time"

	"fairchance/watcher/internal/eventsource"
)

const (
	perChatGap       = 10 * time.Second
	totalPerMinute   = 30 // reads for all strangers together
	repliesPerMinute = 60 // sends to all strangers together
	window           = time.Minute
	busyText         = "Busy. Try again in a minute."
	refusedText      = "I check only Fair Chance coins on testnet."
)

// PublicCheck lets any chat run /check on a coin of one factory. It holds the rate limits, the back off
// after an RPC 429 and counts. It never stores message text or chat IDs past the limit window.
type PublicCheck struct {
	factory string
	pageURL string
	now     func() time.Time

	mu              sync.Mutex
	lastReplyByChat map[int64]time.Time
	replies         []time.Time // sends to strangers in the last minute
	recent          []time.Time // starts of reads in the last minute
	backoffTo       time.Time
	counts          PublicCounts
}

var factoryName = regexp.MustCompile(`^([a-z0-9]+[-_])*[a-z0-9]+(\.([a-z0-9]+[-_])*[a-z0-9]+)+$`)

// PublicCounts are numbers only: no chat IDs and no text.
type PublicCounts struct{ Served, Busy, Refused, Silent int }

// NewPublicCheck allows coins that are one name under factory, such as bag.<factory>. now is nil for the clock.
func NewPublicCheck(factory, pageURL string, now func() time.Time) (*PublicCheck, error) {
	if len(factory) > 64 || !factoryName.MatchString(factory) {
		return nil, fmt.Errorf("public factory %q: use a lowercase NEAR account with at least one dot", factory)
	}
	if now == nil {
		now = time.Now
	}
	return &PublicCheck{factory: factory, pageURL: pageURL, now: now, lastReplyByChat: map[int64]time.Time{}}, nil
}

func (p *PublicCheck) Counts() PublicCounts {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.counts
}

func (p *PublicCheck) helpLine() string {
	line := "Send /check and a Fair Chance coin account on testnet, for example /check bag." + p.factory + ", to read its rules."
	if p.pageURL != "" {
		line += " " + p.pageURL
	}
	return line
}

func (p *PublicCheck) allows(coin string) bool {
	name, ok := strings.CutSuffix(coin, "."+p.factory)
	return ok && name != "" && !strings.Contains(name, ".")
}

// admit says whether one read may start now for the chat, and records it when it may.
func (p *PublicCheck) admit() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	now := p.now()
	if now.Before(p.backoffTo) {
		return false
	}
	keep := p.recent[:0]
	for _, at := range p.recent {
		if now.Sub(at) < window {
			keep = append(keep, at)
		}
	}
	p.recent = keep
	if len(p.recent) >= totalPerMinute {
		return false
	}
	p.recent = append(p.recent, now)
	return true
}

// takeReplySlot says whether the bot may send one message to the chat now: one per chat every ten
// seconds and a cap for all strangers together. Beyond that the bot stays silent.
func (p *PublicCheck) takeReplySlot(chat int64) bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	now := p.now()
	for id, at := range p.lastReplyByChat {
		if now.Sub(at) >= perChatGap {
			delete(p.lastReplyByChat, id)
		}
	}
	if _, waiting := p.lastReplyByChat[chat]; waiting {
		p.counts.Silent++
		return false
	}
	keep := p.replies[:0]
	for _, at := range p.replies {
		if now.Sub(at) < window {
			keep = append(keep, at)
		}
	}
	p.replies = keep
	if len(p.replies) >= repliesPerMinute {
		p.counts.Silent++
		return false
	}
	p.lastReplyByChat[chat] = now
	p.replies = append(p.replies, now)
	return true
}

func (p *PublicCheck) startBackoff() {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.backoffTo = p.now().Add(window)
}

func (p *PublicCheck) count(f func(*PublicCounts)) {
	p.mu.Lock()
	defer p.mu.Unlock()
	f(&p.counts)
}

func (b Bot) handlePublicCheck(ctx context.Context, chat int64, fields []string) {
	p := b.Public
	if b.Checker == nil {
		return
	}
	if !p.takeReplySlot(chat) { // no send slot: no read and no reply
		return
	}
	if len(fields) != 2 || !nearAccount.MatchString(fields[1]) {
		p.count(func(c *PublicCounts) { c.Refused++ })
		b.reply(ctx, chat, "Send /check and one coin account, for example /check bag."+p.factory+".")
		return
	}
	coin := fields[1]
	if !p.allows(coin) {
		p.count(func(c *PublicCounts) { c.Refused++ })
		b.reply(ctx, chat, refusedText)
		return
	}
	if !p.admit() {
		p.count(func(c *PublicCounts) { c.Busy++ })
		b.reply(ctx, chat, busyText)
		return
	}
	card, err := b.Checker.Check(ctx, coin)
	switch {
	case errors.Is(err, eventsource.ErrRateLimited):
		p.startBackoff()
		p.count(func(c *PublicCounts) { c.Busy++ })
		b.reply(ctx, chat, busyText)
	case err != nil:
		b.reply(ctx, chat, "I cannot read "+coin+". Check the account name and try again.")
	default:
		p.count(func(c *PublicCounts) { c.Served++ })
		b.reply(ctx, chat, card)
	}
}
