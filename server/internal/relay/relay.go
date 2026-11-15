// Package relay is the instance's Nostr relay: khatru + SQLite + ChatON policies.
// Write policy: local users publish their profile and relay lists; anyone may
// drop a gift wrap for a local user. Gift wraps are only served to their
// recipient (NIP-42 auth), checked on the filter so it also covers live events.
// See spec/federation.md.
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

// KindDeviceLink carries device linking messages (spec protocol § 8). Ephemeral:
// khatru broadcasts it to current listeners and never stores it.
const KindDeviceLink = 24135

// MaxLinkContent bounds a device linking message (SPAKE2 message or encrypted key).
const MaxLinkContent = 4096

// Kinds accepted by this relay (spec §3). NIP-29 kinds come at step 6.
var acceptedKinds = []uint16{
	uint16(nostr.KindProfileMetadata),
	uint16(nostr.KindRelayListMetadata),
	uint16(nostr.KindDMRelayList),
	uint16(nostr.KindGiftWrap),
	KindDeviceLink,
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
	rl.QueryEvents = append(rl.QueryEvents, db.QueryEvents)
	// No CountEvents: counts would leak how many DMs someone receives.

	// Challenge right away so clients can authenticate before asking for their DMs.
	rl.OnConnect = append(rl.OnConnect, khatru.RequestAuth)

	rl.RejectEvent = append(rl.RejectEvent,
		policies.PreventTimestampsInTheFuture(MaxFuture),
		rejectRumor,
		policies.RestrictToSpecifiedKinds(false, acceptedKinds...),
		blocked(cfg.BlockedPubkeys),
		writePolicy(dir),
		// ponytail: fixed limits per IP, make them configurable once real traffic shows the need.
		policies.EventIPRateLimiter(20, time.Minute, 60),
	)
	rl.RejectFilter = append(rl.RejectFilter, giftWrapsOnlyForRecipient, oneLinkChannel)
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
		// From any key: the new device has no account yet (spec protocol § 8).
		if ev.Kind == KindDeviceLink {
			if len(ev.Content) > MaxLinkContent {
				return true, "invalid: device link message too large"
			}
			if len(tagValues(ev, "c")) != 1 {
				return true, "invalid: device link message needs exactly one c tag"
			}
			return false, ""
		}
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

// giftWrapsOnlyForRecipient rejects any filter that could match a gift wrap
// unless the client is authenticated and asks only for its own (#p = itself).
// Filters, not query results, so live broadcasts are covered too.
func giftWrapsOnlyForRecipient(ctx context.Context, f nostr.Filter) (bool, string) {
	if len(f.Kinds) > 0 && !slices.Contains(f.Kinds, nostr.KindGiftWrap) {
		return false, ""
	}
	authed := khatru.GetAuthed(ctx)
	if authed == "" {
		return true, "auth-required: gift wraps are only served to their recipient"
	}
	if p := f.Tags["p"]; len(p) != 1 || p[0] != authed {
		return true, "restricted: you can only ask for gift wraps addressed to you"
	}
	return false, ""
}

// oneLinkChannel: asking for device link messages needs exactly one channel,
// so nobody can listen to every linking in progress.
func oneLinkChannel(_ context.Context, f nostr.Filter) (bool, string) {
	if slices.Contains(f.Kinds, KindDeviceLink) && len(f.Tags["c"]) != 1 {
		return true, "restricted: ask for exactly one device link channel (#c)"
	}
	return false, ""
}

func tagValues(ev *nostr.Event, name string) []string {
	var values []string
	for _, t := range ev.Tags {
		if len(t) > 1 && t[0] == name {
			values = append(values, t[1])
		}
	}
	return values
}

func recipient(ev *nostr.Event) string {
	if t := ev.Tags.Find("p"); len(t) > 1 {
		return t[1]
	}
	return ""
}
