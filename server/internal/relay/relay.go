// Package relay is the instance's Nostr relay: khatru + SQLite + ChatON policies.
// Write policy: local users publish their profile and relay lists; anyone may
// drop a gift wrap for a local user. Gift wraps are only served to their
// recipient (NIP-42 auth). See spec/federation.md.
package relay

import (
	"context"
	"slices"
	"time"

	"github.com/fiatjaf/eventstore/sqlite3"
	"github.com/fiatjaf/khatru"
	"github.com/fiatjaf/khatru/policies"
	"github.com/nbd-wtf/go-nostr"
)

// MaxFuture matches core's MAX_FUTURE_SECS (spec §6).
const MaxFuture = 15 * time.Minute

// Kinds stored by this relay (spec §3). NIP-29 kinds come at step 6.
var storedKinds = []uint16{
	uint16(nostr.KindProfileMetadata),
	uint16(nostr.KindRelayListMetadata),
	uint16(nostr.KindDMRelayList),
	uint16(nostr.KindGiftWrap),
}

type Config struct {
	Name           string
	Description    string
	BlockedPubkeys []string
}

// Directory is the part of the directory the relay needs.
type Directory interface {
	IsLocal(pubkey string) bool
}

func New(cfg Config, db *sqlite3.SQLite3Backend, dir Directory) *khatru.Relay {
	rl := khatru.NewRelay()
	rl.Info.Name = cfg.Name
	rl.Info.Description = cfg.Description
	rl.Info.Software = "https://github.com/ErwanHeschung/ChatON"
	rl.Info.SupportedNIPs = []any{1, 11, 17, 42, 59, 65} // what we actually do, not khatru's defaults

	rl.StoreEvent = append(rl.StoreEvent, db.SaveEvent)
	rl.ReplaceEvent = append(rl.ReplaceEvent, db.ReplaceEvent)
	// No DeleteEvent: kind 5 (NIP-09) isn't accepted before step 6.
	rl.QueryEvents = append(rl.QueryEvents, onlyOwnGiftWraps(db.QueryEvents))
	// No CountEvents: counts would leak how many DMs someone receives.

	// Challenge right away so clients can authenticate before asking for their DMs.
	rl.OnConnect = append(rl.OnConnect, khatru.RequestAuth)

	rl.RejectEvent = append(rl.RejectEvent,
		policies.PreventTimestampsInTheFuture(MaxFuture),
		rejectRumor,
		policies.RestrictToSpecifiedKinds(false, storedKinds...),
		blocked(cfg.BlockedPubkeys),
		writePolicy(dir),
		// ponytail: fixed limits per IP, make them configurable once real traffic shows the need.
		policies.EventIPRateLimiter(20, time.Minute, 60),
	)
	rl.RejectConnection = append(rl.RejectConnection, policies.ConnectionRateLimiter(10, time.Minute, 30))
	return rl
}

// A kind 14 is a rumor: it only ever travels inside a seal (NIP-17, spec §5).
func rejectRumor(_ context.Context, ev *nostr.Event) (bool, string) {
	if ev.Kind == nostr.KindDirectMessage {
		return true, "invalid: kind 14 must never be published, wrap it (NIP-17)"
	}
	return false, ""
}

func blocked(pubkeys []string) func(context.Context, *nostr.Event) (bool, string) {
	return func(_ context.Context, ev *nostr.Event) (bool, string) {
		if slices.Contains(pubkeys, ev.PubKey) {
			return true, "blocked: this key is blocked on this instance"
		}
		return false, ""
	}
}

func writePolicy(dir Directory) func(context.Context, *nostr.Event) (bool, string) {
	return func(_ context.Context, ev *nostr.Event) (bool, string) {
		if ev.Kind == nostr.KindGiftWrap {
			if !dir.IsLocal(recipient(ev)) {
				return true, "restricted: recipient is not a user of this instance"
			}
			return false, ""
		}
		if !dir.IsLocal(ev.PubKey) {
			return true, "restricted: claim a handle on this instance first"
		}
		return false, ""
	}
}

type queryFunc = func(context.Context, nostr.Filter) (chan *nostr.Event, error)

// onlyOwnGiftWraps drops kind 1059 events unless the authenticated client is their recipient.
func onlyOwnGiftWraps(query queryFunc) queryFunc {
	return func(ctx context.Context, filter nostr.Filter) (chan *nostr.Event, error) {
		in, err := query(ctx, filter)
		if err != nil {
			return nil, err
		}
		authed := khatru.GetAuthed(ctx)
		out := make(chan *nostr.Event)
		go func() {
			defer close(out)
			for ev := range in {
				if ev.Kind == nostr.KindGiftWrap && (authed == "" || recipient(ev) != authed) {
					continue
				}
				out <- ev
			}
		}()
		return out, nil
	}
}

func recipient(ev *nostr.Event) string {
	if t := ev.Tags.Find("p"); len(t) > 1 {
		return t[1]
	}
	return ""
}
