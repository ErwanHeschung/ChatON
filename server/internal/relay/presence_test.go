package relay

import (
	"testing"
	"time"

	"fiatjaf.com/nostr"

	"github.com/ErwanHeschung/ChatON/server/internal/presence"
)

// Step 12k (spec § 12): presence from authenticated locals, in memory, gone
// with the connection; community presence only from and for members.
func TestPresence(t *testing.T) {
	alice, bob, carol, signal := newUser(), newUser(), newUser(), newUser() // carol: another instance
	url := startRelay(t, alice, bob)
	at := func(s int) time.Time { return time.Now().Add(time.Duration(s) * time.Second) }
	status := func(u user, s string, at time.Time, extra ...nostr.Tag) nostr.Event {
		ev := nostr.Event{Kind: presence.Kind, Tags: append(nostr.Tags{{"status", s}, {"device", "d1"}}, extra...), CreatedAt: nostr.Timestamp(at.Unix())}
		_ = ev.Sign(u.sk)
		return ev
	}
	seen := func(r *nostr.Relay, f nostr.Filter) int {
		f.Kinds = []nostr.Kind{presence.Kind}
		evs, _ := query(t, r, f)
		return len(evs)
	}
	of := nostr.Filter{Authors: []nostr.PubKey{signal.pk}}

	mustReject(t, connect(t, url, nil), status(signal, "online", at(0)), "presence without auth")
	mustReject(t, connect(t, url, &carol), status(signal, "online", at(0)), "presence from another instance's user")
	a := connect(t, url, &alice)
	mustReject(t, a, status(signal, "away", at(0)), "an unknown status")
	mustReject(t, a, status(signal, "online", at(-200)), "an old signal")
	mustPublish(t, a, status(signal, "online", at(0)))
	// Anyone who knows the presence key reads it; it's not stored.
	if n := seen(connect(t, url, nil), of); n != 1 {
		t.Fatalf("bob sees %d signals, want 1", n)
	}
	// Alice leaves: her signal goes with her connection.
	a.Close()
	deadline := time.Now().Add(3 * time.Second)
	for seen(connect(t, url, nil), of) != 0 {
		if time.Now().After(deadline) {
			t.Fatal("presence outlives its connection")
		}
		time.Sleep(50 * time.Millisecond)
	}

	// In a community: signed by the member, as themselves; read by members only.
	a = connect(t, url, &alice)
	b := connect(t, url, &bob)
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, at(0)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustReject(t, b, status(alice, "dnd", at(0), h(communityID)), "alice's presence sent by bob")
	mustReject(t, connect(t, url, &carol), status(carol, "online", at(0), h(communityID)), "a non-member's presence")
	mustPublish(t, a, status(alice, "dnd", at(0), h(communityID)))
	in := nostr.Filter{Tags: nostr.TagMap{"h": {communityID}}}
	if n := seen(b, in); n != 1 {
		t.Fatalf("a member sees %d, want 1", n)
	}
	if n := seen(connect(t, url, &carol), in); n != 0 {
		t.Fatalf("a non-member sees %d, want 0", n)
	}
	if n := seen(connect(t, url, nil), in); n != 0 {
		t.Fatalf("an anonymous reader sees %d, want 0", n)
	}
	// Asked by author rather than by community: still members only.
	byAlice := nostr.Filter{Authors: []nostr.PubKey{alice.pk}}
	if n := seen(connect(t, url, &carol), byAlice); n != 0 {
		t.Fatalf("a non-member sees %d by author, want 0", n)
	}
	if n := seen(b, byAlice); n != 1 {
		t.Fatalf("a member sees %d by author, want 1", n)
	}
}
