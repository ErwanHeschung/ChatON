package relay

import (
	"context"
	"errors"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"fiatjaf.com/nostr"

	"github.com/ErwanHeschung/ChatON/server/internal/rules"
)

// handles: what a NIP-05 check would answer, handle → (key, domain).
type handles map[string]user

func (h handles) Instance(_ context.Context, handle string, pk nostr.PubKey) (string, error) {
	if u, ok := h[handle]; ok && u.pk == pk {
		_, domain, _ := strings.Cut(handle, "@")
		return domain, nil
	}
	return "", errors.New("not this key")
}

func TestInstanceRules(t *testing.T) {
	alice, spammer, bob, mallory, dave, eve := newUser(), newUser(), newUser(), newUser(), newUser(), newUser()
	limits := rules.DefaultLimits
	limits.WrapsPerRecipientPerMinute = 2
	r := rules.Fixed(&rules.Rules{Instances: []string{"bad.example"}, Keys: []nostr.PubKey{spammer.pk}, Limits: limits})
	v := handles{"bob@good.example": bob, "mallory@bad.example": mallory, "dave@bad.example": dave}
	url, _ := serveRules(t, filepath.Join(t.TempDir(), "r.bolt"), nostr.Generate(), r, v, alice, spammer)
	now := time.Now()

	a := connect(t, url, &alice)
	mustReject(t, connect(t, url, &spammer), event(spammer, nostr.KindProfileMetadata, nil, now), "blocked key")
	// My blocks (10000): local users only.
	mustPublish(t, a, event(alice, nostr.KindMuteList, nil, now))
	mustReject(t, connect(t, url, &bob), event(bob, nostr.KindMuteList, nil, now), "10000 from another instance")

	// Gift wraps: 2 per minute for Alice, then refused.
	anon := connect(t, url, nil)
	for i := range 3 {
		err := publish(t, anon, event(newUser(), nostr.KindGiftWrap, nostr.Tags{{"p", alice.pk.Hex()}}, now.Add(-time.Duration(i)*time.Second)))
		if (err == nil) != (i < 2) {
			t.Errorf("gift wrap %d: %v", i, err)
		}
	}

	// Communities: members from a blocked instance don't get in.
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, now))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, now))
	join := func(u user, handle string) nostr.Event {
		tags := nostr.Tags{h(communityID), {"code", "AB12CD34"}}
		if handle != "" {
			tags = append(tags, nostr.Tag{"nip05", handle})
		}
		return event(u, nostr.KindSimpleGroupJoinRequest, tags, now)
	}
	mustPublish(t, connect(t, url, &bob), join(bob, "bob@good.example"))
	mustReject(t, connect(t, url, &mallory), join(mallory, "mallory@bad.example"), "join from a blocked instance")
	mustReject(t, connect(t, url, &dave), join(dave, "mallory@bad.example"), "join with someone else's handle")
	mustPublish(t, connect(t, url, &eve), join(eve, "")) // blacklist: unknown instance is fine

	// Whitelist: only good.example, and no unknown instance.
	r2 := rules.Fixed(&rules.Rules{Whitelist: true, Instances: []string{"good.example"}, Limits: rules.DefaultLimits})
	url2, _ := serveRules(t, filepath.Join(t.TempDir(), "r2.bolt"), nostr.Generate(), r2, v, alice)
	a2 := connect(t, url2, &alice)
	mustPublish(t, a2, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, now))
	mustPublish(t, a2, event(alice, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, now))
	mustPublish(t, connect(t, url2, &bob), join(bob, "bob@good.example"))
	mustReject(t, connect(t, url2, &eve), join(eve, ""), "whitelist: unknown instance")
	mustReject(t, connect(t, url2, &dave), join(dave, "dave@bad.example"), "whitelist: unlisted instance")
}
