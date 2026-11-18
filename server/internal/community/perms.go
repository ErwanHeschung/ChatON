package community

import (
	"errors"
	"fmt"
	"regexp"
	"slices"

	"fiatjaf.com/nostr"
)

// Permissions, in the order of spec/protocol.md § 9 (a bit each). Unknown names are ignored.
var Permissions = []string{
	"administrator", "manage_community", "manage_channels", "manage_roles", "manage_invites",
	"create_invite", "kick_members", "ban_members", "moderate_members", "view_audit_log",
	"manage_messages", "mention_everyone", "view_channel", "send_messages", "attach_files",
	"add_reactions", "connect", "speak", "stream", "mute_members", "move_members",
}

// Perms is a set of permissions, one bit per entry of Permissions.
type Perms uint32

// AllPerms: the owner's, and an administrator's.
var AllPerms = Perms(1)<<len(Permissions) - 1

// Perm returns the bit of name, 0 if unknown.
func Perm(name string) Perms {
	if i := slices.Index(Permissions, name); i >= 0 {
		return 1 << i
	}
	return 0
}

func permsOf(names []string) Perms {
	var p Perms
	for _, n := range names {
		p |= Perm(n)
	}
	return p
}

// Has: every permission in names.
func (p Perms) Has(names ...string) bool {
	want := permsOf(names)
	return p&want == want
}

// Names lists the permissions of p, in spec order.
func (p Perms) Names() []string {
	var names []string
	for i, n := range Permissions {
		if p&(1<<i) != 0 {
			names = append(names, n)
		}
	}
	return names
}

const Everyone = "everyone"

// Role is a `["role", id, name, color, attr…]` tag of 39003; attrs are permissions, hoist, mentionable.
type Role struct {
	ID, Name, Color string
	Attrs           []string
}

func (r Role) Perms() Perms { return permsOf(r.Attrs) }

// Policy is what a community decides with: its owner and its roles, highest first, everyone last.
type Policy struct {
	Owner nostr.PubKey
	Roles []Role
}

// Member is who asks: a key, its role ids, its timeout (0: none).
type Member struct {
	PubKey  nostr.PubKey
	Roles   []string
	Timeout nostr.Timestamp
}

// Override is an `["allow"|"deny", target, perm…]` tag of a channel; target is a role id, everyone or a key.
type Override struct {
	Allow  bool
	Target string
	Perms  Perms
}

// DefaultRoles: a new community's, and an older one's (spec § 9).
func DefaultRoles() []Role {
	return []Role{
		{ID: RoleAdmin, Name: "Admin", Attrs: []string{"hoist", "administrator"}},
		{ID: RoleModerator, Name: "Modérateur", Attrs: []string{
			"hoist", "manage_messages", "kick_members", "ban_members", "moderate_members",
			"create_invite", "view_audit_log", "mute_members", "move_members",
		}},
		{ID: Everyone, Name: "@everyone", Attrs: []string{
			"view_channel", "send_messages", "attach_files", "add_reactions", "connect", "speak", "stream",
		}},
	}
}

// RoleTags writes the roles as 39003 tags.
func RoleTags(roles []Role) nostr.Tags {
	tags := make(nostr.Tags, 0, len(roles))
	for _, r := range roles {
		tags = append(tags, append(nostr.Tag{"role", r.ID, r.Name, r.Color}, r.Attrs...))
	}
	return tags
}

// ParseRoles reads the role tags; nil unless they are in this format (with everyone last).
func ParseRoles(tags nostr.Tags) []Role {
	var roles []Role
	for t := range tags.FindAll("role") {
		if len(t) < 4 {
			return nil // NIP-29 ["role", name, description]: before ChatON roles
		}
		roles = append(roles, Role{ID: t[1], Name: t[2], Color: t[3], Attrs: slices.Clone(t[4:])})
	}
	if len(roles) == 0 || roles[len(roles)-1].ID != Everyone {
		return nil
	}
	return roles
}

// ParseOverrides reads a channel's allow and deny tags.
func ParseOverrides(tags nostr.Tags) []Override {
	var out []Override
	for _, t := range tags {
		if len(t) >= 2 && (t[0] == "allow" || t[0] == "deny") {
			out = append(out, Override{Allow: t[0] == "allow", Target: t[1], Perms: permsOf(t[2:])})
		}
	}
	return out
}

func (p *Policy) role(id string) (Role, bool) {
	i := slices.IndexFunc(p.Roles, func(r Role) bool { return r.ID == id })
	if i < 0 {
		return Role{}, false
	}
	return p.Roles[i], true
}

// Effective: m's permissions in the community (channel nil) or in a channel
// with these overrides, at now (spec § 9, permissions.json).
func (p *Policy) Effective(m Member, channel []Override, now nostr.Timestamp) Perms {
	if m.PubKey == p.Owner {
		return AllPerms
	}
	base := Perms(0)
	if r, ok := p.role(Everyone); ok {
		base = r.Perms()
	}
	for _, id := range m.Roles {
		if r, ok := p.role(id); ok && id != Everyone {
			base |= r.Perms()
		}
	}
	if base.Has("administrator") {
		return AllPerms
	}
	if channel != nil {
		apply := func(match func(string) bool) {
			var allow, deny Perms
			for _, o := range channel {
				if match(o.Target) {
					if o.Allow {
						allow |= o.Perms
					} else {
						deny |= o.Perms
					}
				}
			}
			base = base&^deny | allow
		}
		apply(func(t string) bool { return t == Everyone })
		apply(func(t string) bool { return t != Everyone && slices.Contains(m.Roles, t) })
		apply(func(t string) bool { return t == m.PubKey.Hex() })
		if !base.Has("view_channel") {
			return 0
		}
		if !base.Has("connect") {
			base &^= Perm("speak") | Perm("stream")
		}
	}
	if m.Timeout > now {
		return base & Perm("view_channel")
	}
	return base
}

// rank: the position of m's highest role, lower is higher; the owner is above all (-1).
func (p *Policy) rank(m Member) int {
	if m.PubKey == p.Owner {
		return -1
	}
	best := len(p.Roles) // everyone's, or below if there is none
	for i, r := range p.Roles {
		if r.ID == Everyone {
			best = min(best, i)
		} else if slices.Contains(m.Roles, r.ID) {
			return i
		}
	}
	return best
}

// Outranks: a is strictly above b.
func (p *Policy) Outranks(a, b Member) bool {
	return p.rank(a) < p.rank(b)
}

// RoleBelow: role id is strictly under a's highest role.
func (p *Policy) RoleBelow(a Member, id string) bool {
	i := slices.IndexFunc(p.Roles, func(r Role) bool { return r.ID == id })
	return i >= 0 && id != Everyone && p.rank(a) < i
}

var (
	roleIDRe = regexp.MustCompile(`^[a-z0-9_-]{1,32}$`)
	colorRe  = regexp.MustCompile(`^#[0-9a-f]{6}$`)
)

// MaxRoles per community, everyone included.
const MaxRoles = 100

func (r Role) equal(o Role) bool {
	return r.ID == o.ID && r.Name == o.Name && r.Color == o.Color && slices.Equal(r.Attrs, o.Attrs)
}

// CheckRoles: actor may replace p.Roles with next (spec § 9, Modifier les rôles).
// Roles at or above the actor's own don't change; roles below it may be added,
// edited, moved or removed, but gain only permissions the actor has.
func (p *Policy) CheckRoles(actor Member, next []Role) error {
	if len(next) == 0 || next[len(next)-1].ID != Everyone {
		return errors.New("invalid: everyone must be the last role")
	}
	if len(next) > MaxRoles {
		return fmt.Errorf("invalid: at most %d roles", MaxRoles)
	}
	seen := map[string]bool{}
	for _, r := range next {
		switch {
		case !roleIDRe.MatchString(r.ID):
			return fmt.Errorf("invalid: role id %q", r.ID)
		case seen[r.ID]:
			return fmt.Errorf("invalid: role %q twice", r.ID)
		case r.Name == "" || len(r.Name) > 100:
			return fmt.Errorf("invalid: role %q needs a name of 1 to 100 bytes", r.ID)
		case r.Color != "" && !colorRe.MatchString(r.Color):
			return fmt.Errorf("invalid: role %q color must be #rrggbb", r.ID)
		}
		seen[r.ID] = true
	}
	can := p.Effective(actor, nil, nostr.Now())
	if !can.Has("manage_roles") {
		return errors.New("restricted: editing roles needs manage_roles")
	}
	// Up to and including the actor's highest role: as they are, where they are.
	for i := 0; i <= p.rank(actor) && i < len(p.Roles); i++ {
		if i >= len(next) || !next[i].equal(p.Roles[i]) {
			return fmt.Errorf("restricted: role %q is not below yours", p.Roles[i].ID)
		}
	}
	for _, r := range next {
		before, _ := p.role(r.ID)
		if gained := r.Perms() &^ before.Perms(); gained&^can != 0 {
			return fmt.Errorf("restricted: you can't give %v", (gained &^ can).Names())
		}
	}
	return nil
}
