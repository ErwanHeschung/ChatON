package community

import (
	"encoding/json"
	"os"
	"slices"
	"testing"

	"fiatjaf.com/nostr"
)

type permVectors struct {
	Permissions []string   `json:"permissions"`
	Owner       string     `json:"owner"`
	Roles       nostr.Tags `json:"roles"`
	Effective   []struct {
		Name    string     `json:"name"`
		Member  string     `json:"member"`
		Roles   []string   `json:"roles"`
		Timeout int64      `json:"timeout"`
		Now     int64      `json:"now"`
		Channel nostr.Tags `json:"channel"`
		Expect  []string   `json:"expect"`
	} `json:"effective"`
	Outranks []struct {
		Name string `json:"name"`
		A    struct {
			PubKey string   `json:"pubkey"`
			Roles  []string `json:"roles"`
		} `json:"a"`
		B struct {
			PubKey string   `json:"pubkey"`
			Roles  []string `json:"roles"`
		} `json:"b"`
		Expect bool `json:"expect"`
	} `json:"outranks"`
}

// spec/test-vectors/permissions.json: the same cases core (Rust) runs.
func TestPermissionVectors(t *testing.T) {
	raw, err := os.ReadFile("../../../spec/test-vectors/permissions.json")
	if err != nil {
		t.Fatal(err)
	}
	var v permVectors
	if err := json.Unmarshal(raw, &v); err != nil {
		t.Fatal(err)
	}
	if !slices.Equal(v.Permissions, Permissions) {
		t.Fatalf("permissions: %v, want %v", Permissions, v.Permissions)
	}
	policy := Policy{Owner: nostr.MustPubKeyFromHex(v.Owner), Roles: ParseRoles(v.Roles)}
	if len(policy.Roles) != len(v.Roles) {
		t.Fatalf("roles: parsed %d of %d", len(policy.Roles), len(v.Roles))
	}
	for _, c := range v.Effective {
		m := Member{PubKey: nostr.MustPubKeyFromHex(c.Member), Roles: c.Roles, Timeout: nostr.Timestamp(c.Timeout)}
		var channel []Override
		if c.Channel != nil {
			channel = ParseOverrides(c.Channel)
		}
		got := policy.Effective(m, channel, nostr.Timestamp(c.Now)).Names()
		if !slices.Equal(got, c.Expect) {
			t.Errorf("%s: %v, want %v", c.Name, got, c.Expect)
		}
	}
	for _, c := range v.Outranks {
		a := Member{PubKey: nostr.MustPubKeyFromHex(c.A.PubKey), Roles: c.A.Roles}
		b := Member{PubKey: nostr.MustPubKeyFromHex(c.B.PubKey), Roles: c.B.Roles}
		if got := policy.Outranks(a, b); got != c.Expect {
			t.Errorf("%s: %v, want %v", c.Name, got, c.Expect)
		}
	}
}

// The escalation matrix of spec § 9 (Modifier les rôles): who may turn the roles into what.
func TestCheckRoles(t *testing.T) {
	owner := nostr.Generate().Public()
	base := append(DefaultRoles()[:2:2],
		Role{ID: "gardien", Name: "Gardien", Color: "#e5a50a", Attrs: []string{"manage_roles", "kick_members"}},
		Role{ID: "aide", Name: "Aide"},
		DefaultRoles()[2])
	p := Policy{Owner: owner, Roles: base}
	who := func(roles ...string) Member { return Member{PubKey: nostr.Generate().Public(), Roles: roles} }
	edit := func(f func([]Role) []Role) []Role {
		next := make([]Role, len(base))
		for i, r := range base {
			next[i] = r
			next[i].Attrs = slices.Clone(r.Attrs)
		}
		return f(next)
	}
	with := func(r Role, attrs ...string) Role { r.Attrs = append(slices.Clone(r.Attrs), attrs...); return r }
	cases := []struct {
		name  string
		actor Member
		next  []Role
		ok    bool
	}{
		{"owner reverses the roles", Member{PubKey: owner}, edit(func(r []Role) []Role {
			return []Role{r[3], r[2], r[1], r[0], r[4]}
		}), true},
		{"admin edits moderator", who("admin"), edit(func(r []Role) []Role { r[1].Name = "Modo"; return r }), true},
		{"admin edits admin", who("admin"), edit(func(r []Role) []Role { r[0].Color = "#ff0000"; return r }), false},
		{"admin adds a role above admin", who("admin"), edit(func(r []Role) []Role {
			return append([]Role{{ID: "roi", Name: "Roi"}}, r...)
		}), false},
		{"admin adds an administrator below", who("admin"), edit(func(r []Role) []Role {
			return slices.Insert(r, 1, Role{ID: "vice", Name: "Vice", Attrs: []string{"administrator"}})
		}), true},
		{"gardien edits moderator", who("gardien"), edit(func(r []Role) []Role { r[1].Name = "x"; return r }), false},
		{"gardien edits his own role", who("gardien"), edit(func(r []Role) []Role { r[2].Color = ""; return r }), false},
		{"gardien removes his own role", who("gardien"), edit(func(r []Role) []Role { return slices.Delete(r, 2, 3) }), false},
		{"gardien removes a role below", who("gardien"), edit(func(r []Role) []Role { return slices.Delete(r, 3, 4) }), true},
		{"gardien lifts a role above himself", who("gardien"), edit(func(r []Role) []Role {
			return []Role{r[0], r[1], r[3], r[2], r[4]}
		}), false},
		{"gardien adds a role below with kick_members", who("gardien"), edit(func(r []Role) []Role {
			return slices.Insert(r, 3, Role{ID: "videur", Name: "Videur", Attrs: []string{"kick_members", "hoist"}})
		}), true},
		{"gardien adds a role below with ban_members", who("gardien"), edit(func(r []Role) []Role {
			return slices.Insert(r, 3, Role{ID: "videur", Name: "Videur", Attrs: []string{"ban_members"}})
		}), false},
		{"gardien gives everyone kick_members", who("gardien"), edit(func(r []Role) []Role { r[4] = with(r[4], "kick_members"); return r }), true},
		{"gardien gives everyone administrator", who("gardien"), edit(func(r []Role) []Role { r[4] = with(r[4], "administrator"); return r }), false},
		{"gardien takes send_messages from everyone", who("gardien"), edit(func(r []Role) []Role {
			r[4].Attrs = slices.DeleteFunc(r[4].Attrs, func(a string) bool { return a == "send_messages" })
			return r
		}), true},
		{"member without manage_roles", who(), edit(func(r []Role) []Role { r[3].Name = "x"; return r }), false},
		{"moderator without manage_roles", who("moderator"), edit(func(r []Role) []Role { r[3].Name = "x"; return r }), false},
		{"everyone not last", Member{PubKey: owner}, edit(func(r []Role) []Role { return []Role{r[4], r[0]} }), false},
		{"everyone removed", Member{PubKey: owner}, edit(func(r []Role) []Role { return r[:4] }), false},
		{"two roles with one id", Member{PubKey: owner}, edit(func(r []Role) []Role { r[3].ID = "gardien"; return r }), false},
		{"bad id", Member{PubKey: owner}, edit(func(r []Role) []Role { r[3].ID = "Aide !"; return r }), false},
		{"bad color", Member{PubKey: owner}, edit(func(r []Role) []Role { r[3].Color = "red"; return r }), false},
		{"empty name", Member{PubKey: owner}, edit(func(r []Role) []Role { r[3].Name = ""; return r }), false},
	}
	for _, c := range cases {
		if err := p.CheckRoles(c.actor, c.next); (err == nil) != c.ok {
			t.Errorf("%s: %v, want ok=%v", c.name, err, c.ok)
		}
	}
}
