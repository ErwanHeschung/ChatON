// Package relay is the instance's Nostr relay: khatru + bbolt + ChatON policies.
// Write policy: local users publish their profile and relay lists; anyone may
// drop a gift wrap for a local user. Gift wraps are only served to their
// recipient (NIP-42 auth), checked on the filter so it also covers live events.
// See spec/federation.md.
package relay

import (
	"context"
	"slices"
	"strings"
	"sync"
	"time"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/eventstore"
	"fiatjaf.com/nostr/khatru"
	"fiatjaf.com/nostr/khatru/policies"

	"github.com/ErwanHeschung/ChatON/server/internal/community"
	"github.com/ErwanHeschung/ChatON/server/internal/rules"
)

// MaxQueryLimit caps each query; clients page with `until` (spec federation § 5).
const MaxQueryLimit = 500

// MaxFuture matches core's MAX_FUTURE_SECS (spec §6).
const MaxFuture = 15 * time.Minute

// KindDeviceLink carries device linking messages (spec protocol § 8). Ephemeral:
// khatru broadcasts it to current listeners and never stores it.
const KindDeviceLink nostr.Kind = 24135

// MaxLinkContent bounds a device linking message (SPAKE2 message or encrypted key).
const MaxLinkContent = 4096

// Kinds accepted by this relay (spec §3). Group state (39000+) is only signed by the relay itself.
var acceptedKinds = append([]nostr.Kind{
	nostr.KindProfileMetadata,
	nostr.KindRelayListMetadata,
	nostr.KindDMRelayList,
	nostr.KindSimpleGroupList,
	nostr.KindMuteList, // my blocks, private content (spec protocol § 5)
	nostr.KindGiftWrap,
	KindDeviceLink,
}, community.ClientKinds...)

type Config struct {
	Name        string
	Description string
	// Blocked keys and rate limits (rules.yaml); nil: nothing blocked, default limits.
	Rules *rules.Store
	// Where the instance is (ISO 3166 alpha-2), as its admin says: NIP-11
	// relay_countries, for the clients' network map.
	Countries []string
}

// Directory is the part of the directory the relay needs.
type Directory interface {
	IsLocal(pubkey nostr.PubKey) bool
}

func New(cfg Config, db eventstore.Store, dir Directory, groups *community.Groups) *khatru.Relay {
	rl := khatru.NewRelay()
	rl.Info.Name = cfg.Name
	rl.Info.Description = cfg.Description
	rl.Info.RelayCountries = cfg.Countries
	rl.Info.Software = "https://github.com/ErwanHeschung/ChatON"
	rl.Info.SupportedNIPs = []any{1, 11, 17, 29, 42, 51, 59, 65} // what we actually do, not khatru's defaults

	rl.UseEventstore(db, MaxQueryLimit)
	rl.Count = nil // counts would leak how many DMs someone receives (NIP-45 off)

	// Challenge right away so clients can authenticate before asking for their DMs.
	rl.OnConnect = khatru.RequestAuth

	if cfg.Rules == nil {
		cfg.Rules = rules.Fixed(&rules.Rules{Limits: rules.DefaultLimits})
	}
	limits := cfg.Rules.Get().Limits
	rl.OnEvent = policies.SeqEvent(
		policies.PreventTimestampsInTheFuture(MaxFuture),
		rejectAmbiguousEscapes,
		rejectRumor,
		policies.RestrictToSpecifiedKinds(false, acceptedKinds...),
		blocked(cfg.Rules),
		writePolicy(dir, groups),
		// Last: only events that passed count. Per IP (localhost is exempt).
		policies.EventIPRateLimiter(limits.EventsPerMinute, time.Minute, limits.EventsBurst),
		wrapsPerRecipient(limits.WrapsPerRecipientPerMinute),
	)
	rl.OnRequest = policies.SeqRequest(giftWrapsOnlyForRecipient, oneLinkChannel, groups.CheckFilter)
	groups.Bind(rl)
	rl.RejectConnection = policies.ConnectionRateLimiter(limits.ConnectionsPerMinute, time.Minute, limits.ConnectionsBurst)
	return rl
}

// rejectAmbiguousEscapes: fiatjaf.com/nostr doesn't escape U+0008 and U+000C when
// computing ids (NIP-01 does), so it would get the id of such an event wrong.
// ChatON clients never produce them (spec § 1): refuse them instead.
func rejectAmbiguousEscapes(_ context.Context, ev nostr.Event) (bool, string) {
	const ambiguous = "\b\f"
	if strings.ContainsAny(ev.Content, ambiguous) {
		return true, "invalid: U+0008 and U+000C are not accepted"
	}
	for _, tag := range ev.Tags {
		for _, v := range tag {
			if strings.ContainsAny(v, ambiguous) {
				return true, "invalid: U+0008 and U+000C are not accepted"
			}
		}
	}
	return false, ""
}

// Kinds 14 and 15 are rumors: they only ever travel inside a seal (NIP-17, spec §5).
func rejectRumor(_ context.Context, ev nostr.Event) (bool, string) {
	if ev.Kind == nostr.KindDirectMessage || ev.Kind == nostr.KindFileMessage {
		return true, "invalid: kinds 14 and 15 must never be published, wrap them (NIP-17)"
	}
	return false, ""
}

func blocked(r *rules.Store) func(context.Context, nostr.Event) (bool, string) {
	return func(_ context.Context, ev nostr.Event) (bool, string) {
		if r.Get().Blocked(ev.PubKey) {
			return true, "blocked: this key is blocked on this instance"
		}
		return false, ""
	}
}

// wrapsPerRecipient caps the gift wraps one user gets per minute: their sender is
// a throwaway key, so this is the only brake on DM spam. Fixed one-minute windows.
func wrapsPerRecipient(perMinute int) func(context.Context, nostr.Event) (bool, string) {
	var mu sync.Mutex
	window := time.Now()
	counts := map[nostr.PubKey]int{}
	return func(_ context.Context, ev nostr.Event) (bool, string) {
		if ev.Kind != nostr.KindGiftWrap || perMinute <= 0 {
			return false, ""
		}
		pk, _ := recipient(ev)
		mu.Lock()
		defer mu.Unlock()
		if time.Since(window) > time.Minute {
			window, counts = time.Now(), map[nostr.PubKey]int{}
		}
		if counts[pk] >= perMinute {
			return true, "rate-limited: too many messages for this user, try again in a minute"
		}
		counts[pk]++
		return false, ""
	}
}

func writePolicy(dir Directory, groups *community.Groups) func(context.Context, nostr.Event) (bool, string) {
	return func(ctx context.Context, ev nostr.Event) (bool, string) {
		// From any key the community rules allow: members can come from any instance.
		if community.IsGroupKind(ev.Kind) {
			return groups.Check(ctx, ev)
		}
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
			if pk, ok := recipient(ev); !ok || !dir.IsLocal(pk) {
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
	if len(khatru.GetAllAuthed(ctx)) == 0 {
		return true, "auth-required: gift wraps are only served to their recipient"
	}
	p := f.Tags["p"]
	if len(p) != 1 {
		return true, "restricted: you can only ask for gift wraps addressed to you"
	}
	if pk, err := nostr.PubKeyFromHex(p[0]); err != nil || !khatru.IsAuthed(ctx, pk) {
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

func tagValues(ev nostr.Event, name string) []string {
	var values []string
	for t := range ev.Tags.FindAll(name) {
		values = append(values, t[1])
	}
	return values
}

func recipient(ev nostr.Event) (nostr.PubKey, bool) {
	if t := ev.Tags.Find("p"); t != nil {
		pk, err := nostr.PubKeyFromHex(t[1])
		return pk, err == nil
	}
	return nostr.ZeroPK, false
}
