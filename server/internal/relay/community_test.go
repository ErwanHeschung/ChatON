package relay

import (
	"context"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"testing"
	"time"

	"fiatjaf.com/nostr"

	"github.com/ErwanHeschung/ChatON/server/internal/community"
)

const (
	communityID = "k3j9x2m8q0aa"
	channelID   = "g7w2p5n1c4bb"
)

func h(id string) nostr.Tag { return nostr.Tag{"h", id} }

// channel: u's 9007 creating channel id in the test community (spec § 9).
func channel(u user, id, name string, at time.Time, extra ...nostr.Tag) nostr.Event {
	return event(u, nostr.KindSimpleGroupCreateGroup, append(nostr.Tags{h(id), {"parent", communityID}, {"name", name}}, extra...), at)
}

// settings: what a community 9002 carries besides name and channels, its owner and the default roles.
func settings(owner user, tags ...nostr.Tag) nostr.Tags {
	return append(append(tags, nostr.Tag{"owner", owner.pk.Hex()}), community.RoleTags(community.DefaultRoles())...)
}

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
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, settings(alice, h(communityID), nostr.Tag{"name", "Les chatons"}), at(0)))
	if ev := state(t, url, nostr.KindSimpleGroupAdmins, communityID); !hasP(ev, alice.pk, "admin") || ev.PubKey != key.Public() {
		t.Fatalf("admins: %v by %s", ev.Tags, ev.PubKey)
	}
	if meta := state(t, url, nostr.KindSimpleGroupMetadata, communityID); meta.Tags.Find("name")[1] != "Les chatons" || !meta.Tags.Has("private") || !meta.Tags.Has("closed") {
		t.Fatalf("metadata: %v", meta.Tags)
	}
	mustReject(t, c, event(carol, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h("zzzzzzzzzzzz")}, at(0)), "community by a key without handle")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, at(0)), "same id twice")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupMetadata, nostr.Tags{{"d", communityID}}, at(0)), "group state from a client")

	// A channel: one 9007, the relay lists it in the community.
	mustPublish(t, a, channel(alice, channelID, "general", at(1)))
	if meta := state(t, url, nostr.KindSimpleGroupMetadata, communityID); meta.Tags.Find("child")[1] != channelID {
		t.Fatalf("children: %v", meta.Tags)
	}
	// Bob (local, not admin) can't add a channel to Alice's community.
	mustReject(t, b, channel(bob, "bbbbbbbbbbbb", "mine", at(1)), "channel by a non-admin")

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
	mustReject(t, c, event(carol, nostr.KindSimpleGroupEditMetadata, settings(alice, h(communityID), nostr.Tag{"name", "À moi"}), at(4)), "member renames")
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupRemoveUser, nostr.Tags{h(communityID), {"p", carol.pk.Hex()}}, at(4)))
}

// Step 12b: invites and joins only for those who may read them, stored and live (spec § 9, Lecture).
func TestCommunityReads(t *testing.T) {
	alice, bob, carol := newUser(), newUser(), newUser()
	url := startRelay(t, alice, bob, carol)
	a, b, c := connect(t, url, &alice), connect(t, url, &bob), connect(t, url, &carol)
	at := func(s int) time.Time { return time.Now().Add(time.Duration(s) * time.Second) }

	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, at(0)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, c, event(carol, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", carol.pk.Hex(), "moderator"}}, at(1)))
	mustPublish(t, a, channel(alice, channelID, "general", at(1)))

	all := nostr.Filter{Kinds: community.ClientKinds, Tags: nostr.TagMap{"h": {communityID}}}
	seen := func(r *nostr.Relay) (kinds map[nostr.Kind]int, others int) {
		evs, closed := query(t, r, all)
		if closed != "" {
			t.Fatalf("closed: %s", closed)
		}
		kinds = map[nostr.Kind]int{}
		for _, ev := range evs {
			kinds[ev.Kind]++
			if ev.Kind == nostr.KindSimpleGroupJoinRequest && ev.PubKey != bob.pk {
				others++
			}
		}
		return kinds, others
	}
	// Bob, plain member: his own join, no invite, nobody else's join, no moderation.
	if kinds, others := seen(b); kinds[nostr.KindSimpleGroupCreateInvite] != 0 || others != 0 ||
		kinds[nostr.KindSimpleGroupJoinRequest] != 1 || kinds[nostr.KindSimpleGroupPutUser] != 0 {
		t.Fatalf("member reads %v (%d joins of others)", kinds, others)
	}
	// Carol, moderator (view_audit_log): all of it.
	if kinds, others := seen(c); kinds[nostr.KindSimpleGroupCreateInvite] != 1 || others != 1 || kinds[nostr.KindSimpleGroupPutUser] == 0 {
		t.Fatalf("moderator reads %v (%d joins of others)", kinds, others)
	}

	// Live: a new invite reaches Carol, not Bob; the message after it reaches both.
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	live := nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupCreateInvite, nostr.KindSimpleGroupChatMessage}, Tags: nostr.TagMap{"h": {communityID, channelID}}}
	bs, cs := subscribe(ctx, t, b, live), subscribe(ctx, t, c, live)
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "LIVE0001"}}, at(2)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(channelID)}, at(2)))
	for name, s := range map[string]*nostr.Subscription{"bob": bs, "carol": cs} {
		select {
		case ev := <-s.Events:
			if want := map[string]nostr.Kind{"bob": nostr.KindSimpleGroupChatMessage, "carol": nostr.KindSimpleGroupCreateInvite}[name]; ev.Kind != want {
				t.Errorf("%s got %d first, want %d", name, ev.Kind, want)
			}
		case <-ctx.Done():
			t.Fatalf("%s got nothing", name)
		}
	}

	// The relay's own 9000 (Dave joins) goes out live too, without blocking the relay.
	dave := newUser()
	d := connect(t, url, &dave)
	audit := nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupPutUser}, Tags: nostr.TagMap{"h": {communityID}}}
	as := subscribe(ctx, t, c, audit)
	mustPublish(t, d, event(dave, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "LIVE0001"}}, at(3)))
	select {
	case ev := <-as.Events:
		if !hasP(ev, dave.pk) {
			t.Errorf("carol got %v", ev.Tags)
		}
	case <-ctx.Done():
		t.Fatal("carol didn't see dave join")
	}
}

// Step 12c: settings, custom roles, transfer, deletion (spec § 9, Réglages).
func TestCommunitySettings(t *testing.T) {
	alice, bob, carol := newUser(), newUser(), newUser()
	url := startRelay(t, alice, bob, carol)
	a, b, c := connect(t, url, &alice), connect(t, url, &bob), connect(t, url, &carol)
	at := func(s int) time.Time { return time.Now().Add(time.Duration(s) * time.Second) }
	edit := func(owner user, roles []community.Role, extra ...nostr.Tag) nostr.Tags {
		tags := append(nostr.Tags{h(communityID), {"name", "Les chatons"}, {"owner", owner.pk.Hex()}}, community.RoleTags(roles)...)
		return append(tags, extra...)
	}
	roles := func() []string {
		var ids []string
		for tag := range state(t, url, nostr.KindSimpleGroupRoles, communityID).Tags.FindAll("role") {
			ids = append(ids, tag[1])
		}
		return ids
	}
	defaults := community.DefaultRoles()
	gardien := community.Role{ID: "gardien", Name: "Gardien", Color: "#e5a50a", Attrs: []string{"hoist", "kick_members"}}
	withGardien := append(defaults[:2:2], gardien, defaults[2])

	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, at(0)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, c, event(carol, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))

	mustReject(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, nostr.Tags{h(communityID), {"name", "Les chatons"}}, at(1)), "a 9002 without owner nor roles")
	mustReject(t, b, event(bob, nostr.KindSimpleGroupEditMetadata, edit(alice, withGardien), at(1)), "a member adds a role")

	// A Gardien removes members below, not the owner; his role shows in 39001.
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, edit(alice, withGardien), at(1)))
	if got := roles(); !slices.Equal(got, []string{"admin", "moderator", "gardien", "everyone"}) {
		t.Fatalf("roles: %v", got)
	}
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", bob.pk.Hex(), "gardien"}}, at(2)))
	mustReject(t, b, event(bob, nostr.KindSimpleGroupRemoveUser, nostr.Tags{h(communityID), {"p", alice.pk.Hex()}}, at(2)), "gardien removes the owner")
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupRemoveUser, nostr.Tags{h(communityID), {"p", carol.pk.Hex()}}, at(2)))
	if !hasP(state(t, url, nostr.KindSimpleGroupAdmins, communityID), bob.pk, "gardien") {
		t.Fatal("bob isn't gardien in 39001")
	}

	// Removing the role takes it from Bob.
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, edit(alice, defaults), at(3)))
	if hasP(state(t, url, nostr.KindSimpleGroupAdmins, communityID), bob.pk, "gardien") {
		t.Fatal("bob kept a deleted role")
	}

	// Only the owner gives the community away, to a member; then they may leave, Bob may not.
	mustReject(t, b, event(bob, nostr.KindSimpleGroupEditMetadata, edit(bob, defaults), at(4)), "a member takes the community")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, edit(carol, defaults), at(4)), "giving it to a non-member")
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, edit(bob, defaults), at(4)))
	if owner := state(t, url, nostr.KindSimpleGroupMetadata, communityID).Tags.Find("owner"); owner[1] != bob.pk.Hex() {
		t.Fatalf("owner: %v", owner)
	}
	mustReject(t, b, event(bob, nostr.KindSimpleGroupLeaveRequest, nostr.Tags{h(communityID)}, at(5)), "the new owner leaves")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupDeleteGroup, nostr.Tags{h(communityID)}, at(5)), "the former owner deletes it")

	// Deleted: nobody left, the invite no longer opens it.
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupDeleteGroup, nostr.Tags{h(communityID)}, at(6)))
	if members := state(t, url, nostr.KindSimpleGroupMembers, communityID); members.Tags.Find("p") != nil {
		t.Fatalf("members left: %v", members.Tags)
	}
	mustReject(t, c, event(carol, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(7)), "joining a deleted community")
}

// Step 12d: channels and categories (spec § 9, Salons et catégories).
func TestChannels(t *testing.T) {
	alice, bob, carol := newUser(), newUser(), newUser() // carol: another instance
	path, key := filepath.Join(t.TempDir(), "r.bolt"), nostr.Generate()
	url, stop := serve(t, path, key, alice, bob)
	a, b, c := connect(t, url, &alice), connect(t, url, &bob), connect(t, url, &carol)
	at := func(s int) time.Time { return time.Now().Add(time.Duration(s) * time.Second) }
	children := func() (out []string) {
		for tag := range state(t, url, nostr.KindSimpleGroupMetadata, communityID).Tags.FindAll("child") {
			out = append(out, strings.Join(tag[1:], "/"))
		}
		return out
	}
	const mine, theirs, voiceID = "a0000000000a", "c0000000000c", "v0000000000v"

	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, at(0)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, c, event(carol, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", carol.pk.Hex(), "admin"}}, at(0)))

	// Carol, admin from another instance, and Alice each add one: both listed, by the relay.
	mustPublish(t, c, channel(carol, theirs, "carol", at(1)))
	mustPublish(t, a, channel(alice, mine, "alice", at(1)))
	if got := children(); !slices.Equal(got, []string{theirs, mine}) {
		t.Fatalf("children: %v", got)
	}
	mustReject(t, b, channel(bob, "b0000000000b", "bob", at(1)), "channel without manage_channels")
	mustReject(t, a, channel(alice, "x0000000000x", "x", at(1), nostr.Tag{"category", "nope"}), "channel in an unknown category")

	// A 9002 that missed a channel keeps it; arranging needs manage_channels.
	arranged := settings(alice, h(communityID), nostr.Tag{"name", "C"}, nostr.Tag{"child", mine, "texte"}, nostr.Tag{"child", "zzzzzzzzzzzz"},
		nostr.Tag{"category", "texte", "Salons textuels"})
	mustReject(t, b, event(bob, nostr.KindSimpleGroupEditMetadata, arranged, at(2)), "member arranges channels")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, settings(alice, h(communityID), nostr.Tag{"name", "C"},
		nostr.Tag{"child", mine, "nope"}), at(2)), "child in an undeclared category")
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, arranged, at(2)))
	if got := children(); !slices.Equal(got, []string{mine + "/texte", theirs}) {
		t.Fatalf("arranged: %v", got)
	}
	mustPublish(t, a, channel(alice, voiceID, "Vocal", at(2), nostr.Tag{"livekit"}, nostr.Tag{"category", "texte"}))

	// Channel settings: topic and slow mode; its community and type stay.
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, nostr.Tags{h(mine), {"name", "alice"}, {"about", "Sujet"},
		{"parent", communityID}, {"slowmode", "60"}}, at(3)))
	mustReject(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, nostr.Tags{h(mine), {"name", "alice"}, {"parent", communityID}, {"livekit"}}, at(3)), "text becomes voice")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, nostr.Tags{h(mine), {"name", "alice"}}, at(3)), "channel leaves its community")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, nostr.Tags{h(mine), {"name", "alice"}, {"parent", communityID}, {"slowmode", "-1"}}, at(3)), "negative slow mode")
	mustReject(t, b, event(bob, nostr.KindSimpleGroupEditMetadata, nostr.Tags{h(mine), {"name", "bob"}, {"parent", communityID}}, at(3)), "member renames a channel")
	if meta := state(t, url, nostr.KindSimpleGroupMetadata, mine); tagOf(meta, "about") != "Sujet" || tagOf(meta, "slowmode") != "60" {
		t.Fatalf("channel: %v", meta.Tags)
	}

	// Slow mode: Bob waits; Alice (manage_messages) doesn't.
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(mine)}, at(4)))
	if err := publish(t, b, event(bob, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(mine)}, at(4))); err == nil || !strings.Contains(err.Error(), "slow-mode: wait") {
		t.Errorf("second message in slow mode: %v", err)
	}
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(mine)}, at(4)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(mine)}, at(4)))
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(theirs)}, at(4))) // other channel: no wait
	mustReject(t, b, event(bob, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(voiceID)}, at(4)), "message in a voice channel")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(communityID)}, at(4)), "message in the community itself")

	// Deleting a channel takes it off the community.
	mustPublish(t, c, event(carol, nostr.KindSimpleGroupDeleteGroup, nostr.Tags{h(theirs)}, at(5)))
	mustReject(t, c, event(carol, nostr.KindSimpleGroupEditMetadata, nostr.Tags{h(theirs), {"name", "back"}, {"parent", communityID}}, at(5)), "editing a deleted channel")
	want := []string{mine + "/texte", voiceID + "/texte"}
	if got := children(); !slices.Equal(got, want) {
		t.Fatalf("after delete: %v", got)
	}

	// All of it survives a restart.
	stop()
	url, _ = serve(t, path, key, alice, bob)
	if got := children(); !slices.Equal(got, want) {
		t.Fatalf("after restart: %v", got)
	}
	meta := state(t, url, nostr.KindSimpleGroupMetadata, communityID)
	if cat := meta.Tags.Find("category"); cat == nil || cat[2] != "Salons textuels" {
		t.Fatalf("categories after restart: %v", meta.Tags)
	}
	if tagOf(state(t, url, nostr.KindSimpleGroupMetadata, mine), "slowmode") != "60" {
		t.Fatal("slow mode lost on restart")
	}
}

func tagOf(ev nostr.Event, name string) string {
	if t := ev.Tags.Find(name); t != nil {
		return t[1]
	}
	return ""
}

// Step 12e: private channels (spec § 9, Salons privés).
func TestPrivateChannels(t *testing.T) {
	alice, bob, carol := newUser(), newUser(), newUser()
	path, key := filepath.Join(t.TempDir(), "r.bolt"), nostr.Generate()
	url, stop := serve(t, path, key, alice, bob, carol)
	a, b, c := connect(t, url, &alice), connect(t, url, &bob), connect(t, url, &carol)
	at := func(s int) time.Time { return time.Now().Add(time.Duration(s) * time.Second) }
	const staff = "s0000000000s"
	private := nostr.Tags{h(staff), {"name", "staff"}, {"parent", communityID}, {"deny", "everyone", "view_channel"}, {"allow", "moderator", "view_channel"}}

	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, at(0)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, c, event(carol, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", carol.pk.Hex(), "moderator"}}, at(0)))
	mustPublish(t, a, channel(alice, staff, "staff", at(1)))

	// Carol, moderator, can't change channel permissions (no manage_roles); Alice can.
	mustReject(t, c, event(carol, nostr.KindSimpleGroupEditMetadata, private, at(1)), "overrides without manage_roles")
	mustReject(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, append(slices.Clone(private), nostr.Tag{"allow", "everyone", "kick_members"}), at(1)), "a community permission in an override")
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupEditMetadata, private, at(1)))
	secret := event(alice, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(staff)}, at(2))
	mustPublish(t, a, secret)

	inStaff := nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupChatMessage}, Tags: nostr.TagMap{"h": {staff}}}
	staffState := nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupMetadata}, Tags: nostr.TagMap{"d": {staff}}}
	count := func(r *nostr.Relay, f nostr.Filter) int {
		evs, closed := query(t, r, f)
		if closed != "" {
			t.Fatalf("closed: %s", closed)
		}
		return len(evs)
	}
	// Bob, plain member: no history, no name, can't write; Carol, moderator: all of it.
	if n, m := count(b, inStaff), count(b, staffState); n != 0 || m != 0 {
		t.Fatalf("bob reads %d messages and %d states of the private channel", n, m)
	}
	if n, m := count(c, inStaff), count(c, staffState); n != 1 || m != 1 {
		t.Fatalf("carol reads %d messages and %d states", n, m)
	}
	mustReject(t, b, event(bob, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(staff)}, at(2)), "message without view_channel")
	if _, closed := query(t, connect(t, url, nil), staffState); !strings.HasPrefix(closed, "auth-required:") {
		t.Errorf("anonymous state of a private channel: closed %q", closed)
	}

	// Live: the next message reaches Carol, not Bob.
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	bs, cs := subscribe(ctx, t, b, inStaff), subscribe(ctx, t, c, inStaff)
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(staff)}, at(3)))
	select {
	case <-cs.Events:
	case <-ctx.Done():
		t.Fatal("carol got nothing live")
	}
	select {
	case ev := <-bs.Events:
		t.Fatalf("bob got %q live", ev.Content)
	case <-time.After(200 * time.Millisecond):
	}

	// Bob admin: only below himself, only what he has.
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", bob.pk.Hex(), "admin"}}, at(4)))
	if n := count(b, inStaff); n != 2 {
		t.Fatalf("bob, admin, reads %d messages, want the whole history", n)
	}
	mustReject(t, b, event(bob, nostr.KindSimpleGroupEditMetadata, append(slices.Clone(private), nostr.Tag{"deny", "admin", "send_messages"}), at(5)), "override on his own role")
	mustReject(t, b, event(bob, nostr.KindSimpleGroupEditMetadata, append(slices.Clone(private), nostr.Tag{"deny", alice.pk.Hex(), "send_messages"}), at(5)), "override on the owner")
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupEditMetadata, append(slices.Clone(private), nostr.Tag{"allow", carol.pk.Hex(), "send_messages"}), at(5)))

	// It all survives a restart.
	stop()
	url, _ = serve(t, path, key, alice, bob, carol)
	b, c = connect(t, url, &bob), connect(t, url, &carol)
	mustPublish(t, connect(t, url, &alice), event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", bob.pk.Hex()}}, at(6)))
	if n := count(b, inStaff); n != 0 {
		t.Fatalf("after restart, bob (member again) reads %d messages", n)
	}
	meta := state(t, url, nostr.KindSimpleGroupMetadata, communityID)
	if !slices.ContainsFunc(meta.Tags, func(tag nostr.Tag) bool { return len(tag) > 1 && tag[0] == "child" && tag[1] == staff }) {
		t.Fatalf("community lost the private channel: %v", meta.Tags)
	}
	evs, _ := query(t, c, staffState)
	if len(evs) != 1 || !slices.ContainsFunc(evs[0].Tags, func(tag nostr.Tag) bool {
		return len(tag) > 2 && tag[0] == "allow" && tag[1] == carol.pk.Hex() && tag[2] == "send_messages"
	}) {
		t.Fatalf("overrides after restart: %v", evs)
	}
}

// Step 12f: bans, timeouts, the audit log (spec § 9, Modération).
func TestModeration(t *testing.T) {
	alice, bob, carol, dave, eve := newUser(), newUser(), newUser(), newUser(), newUser()
	path, key := filepath.Join(t.TempDir(), "r.bolt"), nostr.Generate()
	url, stop := serve(t, path, key, alice, bob, carol, dave)
	a, b, c, d := connect(t, url, &alice), connect(t, url, &bob), connect(t, url, &carol), connect(t, url, &dave)
	at := func(s int) time.Time { return time.Now().Add(time.Duration(s) * time.Second) }
	const general = "g0000000000g"
	join := func(r *nostr.Relay, u user, s int) nostr.Event {
		return event(u, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(s))
	}
	say := func(u user, s int) nostr.Event {
		return event(u, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(general)}, at(s))
	}
	timeout := func(by user, who user, until int64, s int, extra ...string) nostr.Event {
		return event(by, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), append(nostr.Tag{"p", who.pk.Hex()}, extra...), {"timeout", strconv.FormatInt(until, 10)}}, at(s))
	}
	remove := func(by, who user, s int, ban bool) nostr.Event {
		tags := nostr.Tags{h(communityID), {"p", who.pk.Hex()}}
		if ban {
			tags = append(tags, nostr.Tag{"ban"})
		}
		return event(by, nostr.KindSimpleGroupRemoveUser, tags, at(s))
	}

	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, at(0)))
	mustPublish(t, a, channel(alice, general, "général", at(0)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	for _, m := range []struct {
		r *nostr.Relay
		u user
	}{{b, bob}, {c, carol}, {d, dave}} {
		mustPublish(t, m.r, join(m.r, m.u, 0))
	}
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", bob.pk.Hex(), "moderator"}}, at(0)))

	// Timeouts: moderate_members, below oneself, 28 days at most, no roles given.
	hour := time.Now().Add(time.Hour).Unix()
	mustReject(t, c, timeout(carol, dave, hour, 1), "timeout without moderate_members")
	mustReject(t, b, timeout(bob, alice, hour, 1), "timeout of the owner")
	mustReject(t, b, timeout(bob, carol, time.Now().Add(29*24*time.Hour).Unix(), 1), "timeout past 28 days")
	mustReject(t, b, timeout(bob, carol, hour, 1, "moderator"), "timeout giving roles")
	mustPublish(t, b, timeout(bob, carol, hour, 1))
	if !slices.ContainsFunc(state(t, url, nostr.KindSimpleGroupMembers, communityID).Tags, func(tag nostr.Tag) bool {
		return len(tag) > 2 && tag[0] == "timeout" && tag[1] == carol.pk.Hex()
	}) {
		t.Fatal("39002 doesn't show carol's timeout")
	}
	if !hasP(state(t, url, nostr.KindSimpleGroupMembers, communityID), carol.pk) {
		t.Fatal("a timeout removed carol")
	}
	mustReject(t, c, say(carol, 2), "message while timed out")
	if _, closed := query(t, c, nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupChatMessage}, Tags: nostr.TagMap{"h": {general}}}); closed != "" {
		t.Fatalf("timed out, carol can't read: %s", closed) // view_channel stays
	}
	mustPublish(t, b, timeout(bob, carol, 0, 2))
	mustPublish(t, c, say(carol, 3))

	// Bans: ban_members, also of someone not (yet) a member; a banned key can't come back.
	mustReject(t, d, remove(dave, carol, 4, true), "ban without ban_members")
	ban := remove(bob, carol, 4, true)
	mustPublish(t, b, ban)
	mustReject(t, b, remove(bob, carol, 5, true), "banning twice")
	mustReject(t, c, join(c, carol, 5), "join while banned")
	mustPublish(t, b, remove(bob, eve, 5, true))

	// The log: moderators read it, members don't, nobody erases it.
	audit := nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupRemoveUser}, Tags: nostr.TagMap{"h": {communityID}}}
	if evs, _ := query(t, d, audit); len(evs) != 0 {
		t.Fatalf("dave reads %d moderation events", len(evs))
	}
	if evs, _ := query(t, b, audit); !slices.ContainsFunc(evs, func(ev nostr.Event) bool { return ev.ID == ban.ID }) {
		t.Fatalf("bob doesn't read the ban: %d events", len(evs))
	}
	putBob, _ := query(t, b, nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupPutUser}, Tags: nostr.TagMap{"h": {communityID}}, Authors: []nostr.PubKey{alice.pk}})
	if len(putBob) == 0 {
		t.Fatal("bob doesn't read alice's 9000")
	}
	unban := func(by user, s int, p ...nostr.Tag) nostr.Event {
		return event(by, nostr.KindSimpleGroupDeleteEvent, append(nostr.Tags{h(communityID), {"e", ban.ID.Hex()}, {"k", "9001"}}, p...), at(s))
	}
	mustReject(t, a, event(alice, nostr.KindSimpleGroupDeleteEvent, nostr.Tags{h(communityID), {"e", putBob[0].ID.Hex()}}, at(6)), "erasing the log")
	mustReject(t, d, unban(dave, 6, nostr.Tag{"p", carol.pk.Hex()}), "unban without ban_members")
	mustReject(t, b, unban(bob, 6), "unban without p")
	mustPublish(t, b, unban(bob, 6, nostr.Tag{"p", carol.pk.Hex()}))
	mustPublish(t, c, join(c, carol, 7))

	// Bans and timeouts survive a restart.
	mustPublish(t, b, timeout(bob, dave, hour, 8))
	stop()
	url, _ = serve(t, path, key, alice, bob, carol, dave)
	d = connect(t, url, &dave)
	mustReject(t, d, say(dave, 9), "message while timed out, after restart")
	// Both bans stay in the log; Carol's is lifted, Eve's runs.
	if evs, _ := query(t, connect(t, url, &bob), audit); len(evs) != 2 {
		t.Fatalf("after restart, bob reads %d bans", len(evs))
	}
	mustReject(t, connect(t, url, &carol), remove(carol, dave, 9, true), "carol, plain member, bans")
	if err := publish(t, connect(t, url, &bob), remove(bob, carol, 10, true)); err != nil {
		t.Fatalf("carol's lifted ban still counts after restart: %v", err)
	}
	e := connect(t, url, &eve)
	mustReject(t, e, join(e, eve, 9), "join while banned, after restart")
}

// Step 12g: invites that expire, run out, are revoked (spec § 9, Invitations).
func TestInvites(t *testing.T) {
	alice, bob, carol, dave, eve, fred := newUser(), newUser(), newUser(), newUser(), newUser(), newUser()
	path, key := filepath.Join(t.TempDir(), "r.bolt"), nostr.Generate()
	everyone := []user{alice, bob, carol, dave, eve, fred}
	url, stop := serve(t, path, key, everyone...)
	a, b := connect(t, url, &alice), connect(t, url, &bob)
	at := func(s int) time.Time { return time.Now().Add(time.Duration(s) * time.Second) }
	invite := func(by user, code string, s int, extra ...nostr.Tag) nostr.Event {
		return event(by, nostr.KindSimpleGroupCreateInvite, append(nostr.Tags{h(communityID), {"code", code}}, extra...), at(s))
	}
	join := func(url string, u user, code string, s int) error {
		return publish(t, connect(t, url, &u), event(u, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", code}}, at(s)))
	}
	unix := func(d time.Duration) string { return strconv.FormatInt(time.Now().Add(d).Unix(), 10) }

	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, at(0)))
	mustPublish(t, a, invite(alice, "OPEN0000", 0))
	if err := join(url, bob, "OPEN0000", 0); err != nil {
		t.Fatal(err)
	}
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", bob.pk.Hex(), "moderator"}}, at(0)))

	// Limits are checked when the invite is made.
	mustReject(t, a, invite(alice, "OPEN0000", 1), "a code taken")
	mustReject(t, a, invite(alice, "PAST0000", 1, nostr.Tag{"expiration", unix(-time.Minute)}), "an invite already expired")
	mustReject(t, a, invite(alice, "MANY0000", 1, nostr.Tag{"max_uses", "1001"}), "max_uses over 1000")

	// Expired: refused.
	mustPublish(t, a, invite(alice, "SOON0000", 1, nostr.Tag{"expiration", unix(time.Second)}))
	time.Sleep(1100 * time.Millisecond)
	if join(url, carol, "SOON0000", 2) == nil {
		t.Error("joined with an expired invite")
	}

	// Two uses: the third is refused, also after a restart.
	mustPublish(t, b, invite(bob, "TWO00000", 2, nostr.Tag{"max_uses", "2"}))
	if err := join(url, carol, "TWO00000", 3); err != nil {
		t.Fatal(err)
	}

	// Revoking: the author or manage_invites; carol (member) can't.
	revoke := func(by user, inv nostr.Event, s int) nostr.Event {
		return event(by, nostr.KindSimpleGroupDeleteEvent, nostr.Tags{h(communityID), {"e", inv.ID.Hex()}, {"k", "9009"}}, at(s))
	}
	gone := invite(bob, "GONE0000", 3)
	mustPublish(t, b, gone)
	mustReject(t, connect(t, url, &carol), revoke(carol, gone, 4), "revoking without manage_invites")
	mustPublish(t, a, revoke(alice, gone, 4))
	if join(url, dave, "GONE0000", 5) == nil {
		t.Error("joined with a revoked invite")
	}

	// Who lists what: moderators all of them; carol nothing (no create_invite, none of hers).
	invites := nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupCreateInvite}, Tags: nostr.TagMap{"h": {communityID}}}
	if evs, _ := query(t, b, invites); len(evs) != 3 {
		t.Errorf("bob lists %d invites, want 3 (OPEN, SOON, TWO)", len(evs))
	}
	if evs, _ := query(t, connect(t, url, &carol), invites); len(evs) != 0 {
		t.Errorf("carol lists %d invites", len(evs))
	}

	stop()
	url, _ = serve(t, path, key, everyone...)
	if err := join(url, dave, "TWO00000", 6); err != nil {
		t.Fatalf("second use after restart: %v", err)
	}
	if join(url, eve, "TWO00000", 6) == nil {
		t.Error("third use of a two-use invite, after restart")
	}
	if join(url, fred, "GONE0000", 6) == nil {
		t.Error("revoked invite works again after restart")
	}
	if err := join(url, fred, "OPEN0000", 6); err != nil {
		t.Fatalf("unlimited invite after restart: %v", err)
	}
}

// Step 12i: replies, reactions, pins (spec § 9, Messages).
func TestMessages(t *testing.T) {
	alice, bob, carol := newUser(), newUser(), newUser()
	path, key := filepath.Join(t.TempDir(), "r.bolt"), nostr.Generate()
	url, stop := serve(t, path, key, alice, bob, carol)
	a, b := connect(t, url, &alice), connect(t, url, &bob)
	at := func(s int) time.Time { return time.Now().Add(time.Duration(s) * time.Second) }
	const other = "o0000000000o"
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{h(communityID)}, at(0)))
	mustPublish(t, a, channel(alice, channelID, "general", at(0)))
	mustPublish(t, a, channel(alice, other, "other", at(0)))
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupCreateInvite, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(0)))

	hi := event(alice, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(channelID)}, at(1))
	elsewhere := event(alice, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(other)}, at(1))
	mustPublish(t, a, hi)
	mustPublish(t, a, elsewhere)
	// A reply is a message: the relay takes it as one.
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupChatMessage, nostr.Tags{h(channelID), {"q", hi.ID.Hex(), url, alice.pk.Hex()}, {"p", alice.pk.Hex()}}, at(2)))

	// Reactions: to a message of the channel, once per emoji, with add_reactions.
	react := func(u user, target nostr.ID, emoji string, s int) nostr.Event {
		ev := nostr.Event{Kind: nostr.KindReaction, Tags: nostr.Tags{h(channelID), {"e", target.Hex()}, {"p", alice.pk.Hex()}, {"k", "9"}}, CreatedAt: nostr.Timestamp(at(s).Unix()), Content: emoji}
		_ = ev.Sign(u.sk)
		return ev
	}
	cat := react(bob, hi.ID, "🐱", 2)
	mustPublish(t, b, cat)
	mustReject(t, b, react(bob, hi.ID, "🐱", 3), "the same emoji twice")
	mustPublish(t, b, react(bob, hi.ID, "👍", 3))
	mustReject(t, b, react(bob, elsewhere.ID, "🐱", 3), "a message of another channel")
	mustReject(t, b, react(bob, hi.ID, "", 3), "an empty reaction")
	mustReject(t, b, react(bob, hi.ID, strings.Repeat("x", 65), 3), "a 65-byte reaction")

	// Bob takes his back; carol (a member) can't take another's.
	thumb := react(bob, hi.ID, "🙂", 3)
	mustPublish(t, b, thumb)
	mustPublish(t, connect(t, url, &carol), event(carol, nostr.KindSimpleGroupJoinRequest, nostr.Tags{h(communityID), {"code", "AB12CD34"}}, at(3)))
	mustReject(t, connect(t, url, &carol), event(carol, nostr.KindSimpleGroupDeleteEvent, nostr.Tags{h(channelID), {"e", thumb.ID.Hex()}, {"k", "7"}}, at(4)), "removing another's reaction")
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupDeleteEvent, nostr.Tags{h(channelID), {"e", thumb.ID.Hex()}, {"k", "7"}}, at(4)))
	mustPublish(t, b, event(bob, nostr.KindSimpleGroupDeleteEvent, nostr.Tags{h(channelID), {"e", cat.ID.Hex()}, {"k", "7"}}, at(4)))
	reactions := nostr.Filter{Kinds: []nostr.Kind{nostr.KindReaction}, Tags: nostr.TagMap{"h": {channelID}}}
	if evs, _ := query(t, b, reactions); len(evs) != 1 || evs[0].Content != "👍" {
		t.Errorf("reactions after removing one: %v", evs)
	}
	// Timed out: no reactions.
	until := strconv.FormatInt(time.Now().Add(time.Hour).Unix(), 10)
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", bob.pk.Hex()}, {"timeout", until}}, at(4)))
	mustReject(t, b, react(bob, hi.ID, "🎉", 5), "a reaction while timed out")
	mustPublish(t, a, event(alice, nostr.KindSimpleGroupPutUser, nostr.Tags{h(communityID), {"p", bob.pk.Hex()}, {"timeout", "0"}}, at(5)))

	// Pins: manage_messages, messages of the channel, 50 at most.
	pins := func(u user, s int, ids ...nostr.ID) nostr.Event {
		tags := nostr.Tags{h(channelID)}
		for _, id := range ids {
			tags = append(tags, nostr.Tag{"e", id.Hex()})
		}
		return event(u, community.KindSetPins, tags, at(s))
	}
	mustReject(t, b, pins(bob, 6, hi.ID), "a member pinning")
	mustReject(t, a, pins(alice, 6, elsewhere.ID), "pinning a message of another channel")
	many := make([]nostr.ID, 51)
	for i := range many {
		many[i] = hi.ID
	}
	mustReject(t, a, pins(alice, 6, many...), "51 pins")
	mustPublish(t, a, pins(alice, 6, hi.ID))
	if got := state(t, url, community.KindPins, channelID); got.PubKey != key.Public() || got.Tags.Find("e")[1] != hi.ID.Hex() {
		t.Errorf("39005: %v", got)
	}

	stop()
	url, _ = serve(t, path, key, alice, bob, carol)
	if got := state(t, url, community.KindPins, channelID); got.Tags.Find("e") == nil || got.Tags.Find("e")[1] != hi.ID.Hex() {
		t.Errorf("39005 after restart: %v", got)
	}
}
