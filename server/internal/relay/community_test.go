package relay

import (
	"context"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"fiatjaf.com/nostr"
)

const (
	communityID = "k3j9x2m8q0aa"
	channelID   = "g7w2p5n1c4bb"
)

func h(id string) nostr.Tag { return nostr.Tag{"h", id} }

// state returns the relay-signed group state event of kind for id (public).
func state(t *testing.T, url string, kind nostr.Kind, id string) nostr.Event {
	t.Helper()
	evs, closed := query(t, connect(t, url, nil), nostr.Filter{Kinds: []nostr.Kind{kind}, Tags: nostr.TagMap{"d": {id}}})
	if closed != "" || len(evs) != 1 {
		t.Fatalf("kind %d for %s: %d events, closed %q", kind, id, len(evs), closed)
	}
	return evs[0]
}

func hasP(ev nostr.Event, pk nostr.PubKey, extra ...string) bool {
	for tag := range ev.Tags.FindAll("p") {
		if tag[1] == pk.Hex() && slices.Equal(tag[2:], extra) {
			return true
		}
	}
	return false
}

func mustPublish(t *testing.T, r *nostr.Relay, ev nostr.Event) {
	t.Helper()
	if err := publish(t, r, ev); err != nil {
		t.Fatalf("kind %d rejected: %v", ev.Kind, err)
	}
}

func mustReject(t *testing.T, r *nostr.Relay, ev nostr.Event, what string) {
	t.Helper()
	if err := publish(t, r, ev); err == nil {
		t.Errorf("%s: accepted, want rejected", what)
	}
}

func TestCommunity(t *testing.T) {
	alice, bob, carol := newUser(), newUser(), newUser() // carol: another instance
	path, key := filepath.Join(t.TempDir(), "r.bolt"), nostr.Generate()
	url, stop := serve(t, path, key, alice, bob)
	a, b, c := connect(t, url, &alice), connect(t, url, &bob), connect(t, url, &carol)
	now := time.Now()
	at := func(s int) time.Time { return now.Add(time.Duration(s) * time.Second) }

	// Alice creates the community and becomes its admin.
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, at(0)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, nostr.Tags{h(communityID), {"name", "Les chatons"}}, at(0)))
	if ev := state(t, url, nostr.KindSimpleGroupAdmins, communityID); !hasP(ev, alice.pk, "admin") || ev.PubKey != key.Public() {
		t.Fatalf("admins: %v by %s", ev.Tags, ev.PubKey)
	}
	if meta := state(t, url, nostr.KindSimpleGroupMetadata, communityID); meta.Tags.Find("name")[1] != "Les chatons" || !meta.Tags.Has("private") || !meta.Tags.Has("closed") {
		t.Fatalf("metadata: %v", meta.Tags)
	}
	mustReject(t, c, event(carol, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h("zzzzzzzzzzzz")}, at(0)), "community by a key without handle")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, at(0)), "same id twice")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupMetadata, nostr.Tags{{"d", communityID}}, at(0)), "group state from a client")

	// A channel: create, attach to the community, list it.
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(channelID)}, at(1)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, nostr.Tags{h(channelID), {"name", "general"}, {"parent", communityID}}, at(1)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, nostr.Tags{h(communityID), {"name", "Les chatons"}, {"child", channelID}}, at(1)))
	if meta := state(t, url, nostr.KindSimpleGroupMetadata, communityID); meta.Tags.Find("child")[1] != channelID {
		t.Fatalf("children: %v", meta.Tags)
	}
	// Bob (local, not admin) can't take a channel into Alice's community.
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h("bbbbbbbbbbbb")}, at(1)))
	mustReject(t, b, event(bob, nostr.KindSimpleGroupEditMetadata, nostr.Tags{h("bbbbbbbbbbbb"), {"parent", communityID}}, at(1)), "channel by a non-admin")

	// Outsiders neither read nor write.
	inChannel := nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupChatMessage}, Tags: nostr.TagMap{"h": {channelID}}}
	if _, closed := query(t, b, inChannel); !strings.HasPrefix(closed, "restricted:") {
		t.Errorf("non-member read: closed %q", closed)
	}
	if _, closed := query(t, connect(t, url, nil), inChannel); !strings.HasPrefix(closed, "auth-required:") {
		t.Errorf("anonymous read: closed %q", closed)
	}
	if _, closed := query(t, a, nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupChatMessage}}); !strings.HasPrefix(closed, "restricted:") {
		t.Errorf("messages without #h: closed %q", closed)
	}
	mustReject(t, b, event(bob, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(channelID)}, at(2)), "message by a non-member")

	// Invite: wrong code refused, right code lets Bob and Carol (other instance) in.
	mustReject(t, b, event(bob, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "BOBCODE1"}}, at(2)), "invite by a non-member")
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(2)))
	mustReject(t, b, event(bob, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "WRONG000"}}, at(2)), "join with a wrong code")
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(2)))
	mustPublish(t, c, event(carol, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(2)))
	if members := state(t, url, nostr.KindSimpleGroupMembers, communityID); !hasP(members, bob.pk) || !hasP(members, carol.pk) {
		t.Fatalf("members: %v", members.Tags)
	}

	// Members chat; only their own messages, unless moderator.
	hi := event(alice, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(channelID)}, at(3))
	mustPublish(t, a, hi)
	mustPublish(t, c, event(carol, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(channelID)}, at(3)))
	if evs, closed := query(t, b, inChannel); len(evs) != 2 || closed != "" {
		t.Fatalf("bob reads %d messages, closed %q", len(evs), closed)
	}
	deleteHi := nostr.Tags{h(channelID), {"e", hi.ID.Hex()}}
	mustReject(t, b, event(bob, nostr.KindSimpleGroupDeleteEvent, deleteHi, at(4)), "member deletes someone else's message")
	mustReject(t, b, event(bob, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", bob.pk.Hex(), "admin"}}, at(4)), "self-promotion")
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", bob.pk.Hex(), "moderator"}}, at(4)))
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupDeleteEvent, deleteHi, at(5)))
	if evs, _ := query(t, b, inChannel); len(evs) != 1 {
		t.Fatalf("after delete: %d messages, want 1", len(evs))
	}
	mustReject(t, b, event(bob, nostr.KindSimpleGroupRemoveUser, nostr.Tags{h(communityID), {"p", alice.pk.Hex()}}, at(5)), "moderator removes an admin")

	// Removed: Carol can't post anymore, and her open subscription goes quiet.
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	carolLive, bobLive := subscribe(ctx, t, c, inChannel), subscribe(ctx, t, b, inChannel)
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupRemoveUser, nostr.Tags{h(communityID), {"p", carol.pk.Hex()}}, at(6)))
	mustReject(t, c, event(carol, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(channelID)}, at(7)), "message by a removed member")
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(channelID)}, at(7)))
	select {
	case <-bobLive.Events: // a member still gets it live
	case <-ctx.Done():
		t.Fatal("bob got nothing live")
	}
	select {
	case ev := <-carolLive.Events:
		t.Fatalf("removed member got a live message: %s", ev.Content)
	case <-time.After(200 * time.Millisecond):
	}

	// Restart: the state comes back from the relay-signed events.
	stop()
	url, _ = serve(t, path, key, alice, bob)
	b = connect(t, url, &bob)
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(channelID)}, at(8)))
	c = connect(t, url, &carol)
	mustReject(t, c, event(carol, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(channelID)}, at(8)), "removed member after restart")
	mustPublish(t, c, event(carol, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(8))) // invite codes survive too
	if admins := state(t, url, nostr.KindSimpleGroupAdmins, communityID); !hasP(admins, bob.pk, "moderator") {
		t.Fatalf("admins after restart: %v", admins.Tags)
	}
	// The new state must replace the old one, though earlier updates ran ahead of the clock.
	if members := state(t, url, nostr.KindSimpleGroupMembers, communityID); !hasP(members, carol.pk) {
		t.Fatalf("members after restart: %v", members.Tags)
	}
}

// subscribe opens a live subscription and skips the stored events.
func subscribe(ctx context.Context, t *testing.T, r *nostr.Relay, f nostr.Filter) *nostr.Subscription {
	t.Helper()
	sub, err := r.Subscribe(ctx, f, nostr.SubscriptionOptions{})
	if err != nil {
		t.Fatal(err)
	}
	for {
		select {
		case <-sub.Events:
		case <-sub.EndOfStoredEvents:
			return sub
		case <-ctx.Done():
			t.Fatal("no EOSE")
		}
	}
}

// Step 12a: owner, default roles, hierarchy (spec § 9, Rôles et permissions).
func TestCommunityOwnerAndHierarchy(t *testing.T) {
	alice, bob, carol := newUser(), newUser(), newUser()
	url := startRelay(t, alice, bob, carol)
	a, b, c := connect(t, url, &alice), connect(t, url, &bob), connect(t, url, &carol)
	at := func(s int) time.Time { return time.Now().Add(time.Duration(s) * time.Second) }

	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, at(0)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, c, event(carol, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))

	// State: the owner in 39000, the default roles in 39003, everyone last.
	if meta := state(t, url, nostr.KindSimpleGroupMetadata, communityID); meta.Tags.Find("owner")[1] != alice.pk.Hex() {
		t.Fatalf("owner: %v", meta.Tags)
	}
	var roles []string
	for tag := range state(t, url, nostr.KindSimpleGroupRoles, communityID).Tags.FindAll("role") {
		roles = append(roles, tag[1])
	}
	if !slices.Equal(roles, []string{"admin", "moderator", "everyone"}) {
		t.Fatalf("roles: %v", roles)
	}

	mustReject(t, a, event(alice, nostr.KindSimpleGroupLeaveRequest, nostr.Tags{h(communityID)}, at(1)), "the owner leaves")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", newUser().pk.Hex(), "moderator"}}, at(1)), "roles for a non-member")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", bob.pk.Hex(), "everyone"}}, at(1)), "giving everyone")

	// Bob admin: everything but the owner's, and only below himself.
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", bob.pk.Hex(), "admin"}}, at(1)))
	mustReject(t, b, event(bob, nostr.KindSimpleGroupRemoveUser, nostr.Tags{h(communityID), {"p", alice.pk.Hex()}}, at(2)), "admin removes the owner")
	mustReject(t, b, event(bob, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", carol.pk.Hex(), "admin"}}, at(2)), "admin gives his own rank")
	mustReject(t, b, event(bob, nostr.KindSimpleGroupDeleteGroup, nostr.Tags{h(communityID)}, at(2)), "admin deletes the community")
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", carol.pk.Hex(), "moderator"}}, at(2)))

	// Carol moderator: removes members below her, gives no roles.
	mustReject(t, c, event(carol, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", carol.pk.Hex()}}, at(3)), "moderator manages roles")
	mustReject(t, c, event(carol, nostr.KindSimpleGroupRemoveUser, nostr.Tags{h(communityID), {"p", bob.pk.Hex()}}, at(3)), "moderator removes an admin")
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", carol.pk.Hex()}}, at(3))) // back to member
	mustReject(t, c, event(carol, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "CAROL001"}}, at(4)), "member invites")
	mustReject(t, c, event(carol, nostr.KindSimpleGroupEditMetadata, nostr.Tags{h(communityID), {"name", "À moi"}}, at(4)), "member renames")
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupRemoveUser, nostr.Tags{h(communityID), {"p", carol.pk.Hex()}}, at(4)))
}
