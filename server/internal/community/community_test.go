package community

import (
	"testing"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/eventstore/slicestore"
)

type noDir struct{}

func (noDir) IsLocal(nostr.PubKey) bool { return true }

// A community from before ChatON roles (NIP-29 39003, no owner in 39000) loads
// with the default roles and its creator as owner.
func TestLegacyCommunityGetsDefaultsAndOwner(t *testing.T) {
	key, alice := nostr.Generate(), nostr.Generate()
	store := &slicestore.SliceStore{}
	if err := store.Init(); err != nil {
		t.Fatal(err)
	}
	save := func(k nostr.SecretKey, kind nostr.Kind, tags nostr.Tags) {
		ev := nostr.Event{Kind: kind, CreatedAt: 1700000000, Tags: tags}
		if err := ev.Sign(k); err != nil {
			t.Fatal(err)
		}
		if err := store.SaveEvent(ev); err != nil {
			t.Fatal(err)
		}
	}
	save(alice, nostr.KindSimpleGroupCreateGroup, nostr.Tags{{"h", "k3j9x2m8q0aa"}})
	save(key, nostr.KindSimpleGroupMetadata, nostr.Tags{{"d", "k3j9x2m8q0aa"}, {"name", "Les chatons"}})
	save(key, nostr.KindSimpleGroupAdmins, nostr.Tags{{"d", "k3j9x2m8q0aa"}, {"p", alice.Public().Hex(), "admin"}})
	save(key, nostr.KindSimpleGroupMembers, nostr.Tags{{"d", "k3j9x2m8q0aa"}, {"p", alice.Public().Hex()}})
	save(key, nostr.KindSimpleGroupRoles, nostr.Tags{{"d", "k3j9x2m8q0aa"}, {"role", "admin", "Everything"}, {"role", "moderator", "Some"}})

	g, err := New(key, "wss://chaton.example", store, noDir{})
	if err != nil {
		t.Fatal(err)
	}
	p := g.policies["k3j9x2m8q0aa"]
	switch {
	case p == nil:
		t.Fatal("no policy")
	case p.Owner != alice.Public():
		t.Errorf("owner %s, want the creator", p.Owner.Hex())
	case len(p.Roles) != 3 || p.Roles[2].ID != Everyone:
		t.Errorf("roles %v, want the defaults", p.Roles)
	}
	moderator := p.Effective(Member{PubKey: nostr.Generate().Public(), Roles: []string{RoleModerator}}, nil, 0)
	if !moderator.Has("ban_members", "kick_members", "manage_messages") || moderator.Has("manage_roles") {
		t.Errorf("legacy moderator: %v", moderator.Names())
	}
}
