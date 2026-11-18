// Package community hosts NIP-29 groups: a community is a group, its channels
// are child groups, and members and roles live on the community. The relay
// checks every group event against the permissions of spec/protocol.md § 9
// (perms.go) and publishes each group's state (39000-39003) signed with its own key.
package community

import (
	"context"
	"errors"
	"fmt"
	"iter"
	"maps"
	"os"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"sync"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/eventstore"
	"fiatjaf.com/nostr/khatru"
	"fiatjaf.com/nostr/nip29"

	"github.com/ErwanHeschung/ChatON/server/internal/rules"
)

const (
	RoleAdmin     = "admin"
	RoleModerator = "moderator"
)

var idRe = regexp.MustCompile(`^[a-z0-9]{12}$`)

// ChatON's own kinds for pins (spec § 9, Messages): no NIP has them.
const (
	KindSetPins nostr.Kind = 9010  // a channel's whole pin list, from a moderator
	KindPins    nostr.Kind = 39005 // the same, signed by the relay
	maxPins                = 50
	maxReaction            = 64 // bytes of an emoji
)

// Kinds a client may publish; 39000-39005 only come from the relay.
var ClientKinds = []nostr.Kind{
	nostr.KindSimpleGroupChatMessage,
	nostr.KindReaction,
	KindSetPins,
	nostr.KindSimpleGroupPutUser,
	nostr.KindSimpleGroupRemoveUser,
	nostr.KindSimpleGroupEditMetadata,
	nostr.KindSimpleGroupDeleteEvent,
	nostr.KindSimpleGroupCreateGroup,
	nostr.KindSimpleGroupDeleteGroup,
	nostr.KindSimpleGroupCreateInvite,
	nostr.KindSimpleGroupJoinRequest,
	nostr.KindSimpleGroupLeaveRequest,
}

var stateKinds = []nostr.Kind{
	nostr.KindSimpleGroupMetadata,
	nostr.KindSimpleGroupAdmins,
	nostr.KindSimpleGroupMembers,
	nostr.KindSimpleGroupRoles,
}

// IsGroupKind: an event this package decides on.
func IsGroupKind(k nostr.Kind) bool {
	return slices.Contains(ClientKinds, k) || slices.Contains(stateKinds, k)
}

type Directory interface {
	IsLocal(pubkey nostr.PubKey) bool
}

type Groups struct {
	mu     sync.Mutex
	groups map[string]*nip29.Group
	// last 39000-39003 timestamp per group: kept strictly increasing so a
	// newer state always replaces the previous one (same second = lowest id wins).
	stamps map[string]nostr.Timestamp

	key      nostr.SecretKey
	self     nostr.PubKey
	relayURL string
	store    eventstore.Store
	dir      Directory
	relay    *khatru.Relay

	// Instance rules (spec federation § 6); nil: everyone is welcome.
	rules  *rules.Store
	verify Verifier
	// The instance of members from elsewhere, from their verified join (nip05).
	origins map[nostr.PubKey]string
	// Owner and roles of each community (39000 owner, 39003), by community id.
	policies map[string]*Policy
	// Communities whose stored state predates owner and roles: republished by Bind.
	stale []string
	// Relay events to broadcast once mu is released: PreventBroadcast takes it.
	outbox []nostr.Event
	// Categories and placement (communities), slow mode and limit (channels), by group id.
	layouts map[string]*layout
	// When each member last wrote in each slow-mode channel, by channel id + key.
	// ponytail: in memory, lost on restart (spec § 9 says so); grows with slow-mode writers only.
	spoke map[string]nostr.Timestamp
	// Moderation (spec § 9, Modération), by community id: each banned key's
	// 9001, and each running timeout's end.
	bans     map[string]map[nostr.PubKey]nostr.ID
	timeouts map[string]map[nostr.PubKey]nostr.Timestamp
	// Invite codes of each community, with their limits and uses (spec § 9, Invitations).
	invites map[string]map[string]*invite
}

// Moderation limits (spec § 9, Modération).
const (
	maxReason  = 512
	maxTimeout = 28 * 24 * 3600
)

// unlock releases mu, then broadcasts what was published under it.
func (g *Groups) unlock() {
	out := g.outbox
	g.outbox = nil
	g.mu.Unlock()
	for _, ev := range out {
		g.relay.BroadcastEvent(ev)
	}
}

// Verifier says which instance a handle is on, if it names this key there (NIP-05).
type Verifier interface {
	Instance(ctx context.Context, handle string, pk nostr.PubKey) (string, error)
}

// UseRules applies the instance rules to members from other instances.
func (g *Groups) UseRules(r *rules.Store, v Verifier) {
	g.rules, g.verify = r, v
}

// New loads the groups from their last published state (39000-39003, signed by
// key) and the invite codes (9009).
func New(key nostr.SecretKey, relayURL string, store eventstore.Store, dir Directory) (*Groups, error) {
	g := &Groups{
		groups:   map[string]*nip29.Group{},
		stamps:   map[string]nostr.Timestamp{},
		key:      key,
		self:     key.Public(),
		relayURL: relayURL,
		store:    store,
		dir:      dir,
		origins:  map[nostr.PubKey]string{},
		policies: map[string]*Policy{},
		layouts:  map[string]*layout{},
		spoke:    map[string]nostr.Timestamp{},
		bans:     map[string]map[nostr.PubKey]nostr.ID{},
		timeouts: map[string]map[nostr.PubKey]nostr.Timestamp{},
		invites:  map[string]map[string]*invite{},
	}
	owners, roles := map[string]nostr.PubKey{}, map[string][]Role{}
	// Metadata first: it creates the group the other state events merge into.
	for _, kind := range stateKinds {
		for ev := range g.query(nostr.Filter{Kinds: []nostr.Kind{kind}, Authors: []nostr.PubKey{g.self}}, loadLimit) {
			id := ev.Tags.GetD()
			group := g.groups[id]
			if group == nil {
				if kind != nostr.KindSimpleGroupMetadata {
					continue
				}
				group = g.newGroup(id)
				g.groups[id] = group
			}
			var err error
			switch kind {
			case nostr.KindSimpleGroupMetadata:
				err = group.MergeInMetadataEvent(&ev)
				g.layouts[id] = parseLayout(ev.Tags)
				if t := ev.Tags.Find("owner"); t != nil {
					owners[id], _ = nostr.PubKeyFromHex(t[1])
				}
			case nostr.KindSimpleGroupAdmins:
				err = group.MergeInAdminsEvent(&ev)
			case nostr.KindSimpleGroupMembers:
				err = group.MergeInMembersEvent(&ev)
				for t := range ev.Tags.FindAll("timeout") {
					if pk, perr := nostr.PubKeyFromHex(t[1]); perr == nil && len(t) > 2 {
						until, _ := strconv.ParseInt(t[2], 10, 64)
						g.timeout(id, pk, nostr.Timestamp(until))
					}
				}
			case nostr.KindSimpleGroupRoles:
				err = group.MergeInRolesEvent(&ev)
				roles[id] = ParseRoles(ev.Tags)
			}
			if err != nil {
				return nil, fmt.Errorf("group %s, kind %d: %w", id, kind, err)
			}
			g.stamps[id] = max(g.stamps[id], ev.CreatedAt)
		}
	}
	// Communities from before ChatON roles get the default ones, and their creator as owner.
	for id, group := range g.groups {
		if group.Parent != "" {
			continue
		}
		p := &Policy{Owner: owners[id], Roles: roles[id]}
		if p.Roles == nil || p.Owner == (nostr.PubKey{}) {
			g.stale = append(g.stale, id)
		}
		if p.Roles == nil {
			p.Roles = DefaultRoles()
		}
		if p.Owner == (nostr.PubKey{}) {
			p.Owner = g.creator(id)
		}
		g.policies[id] = p
	}
	// Invites (a revoked one was deleted), then their uses: the joins accepted with them.
	for ev := range g.query(nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupCreateInvite}}, loadLimit) {
		g.addInvite(ev)
	}
	// Joins were verified when accepted: their nip05 says where members come from.
	for ev := range g.query(nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupJoinRequest}}, loadLimit) {
		if inv := g.invites[groupID(ev)][tagValue(ev, "code")]; inv != nil {
			inv.Uses++
		}
		if t := ev.Tags.Find("nip05"); t != nil {
			if _, domain, ok := strings.Cut(t[1], "@"); ok {
				g.origins[ev.PubKey] = rules.Domain(domain)
			}
		}
	}
	// Bans: every 9001 ["ban"], minus those an unban (9005 ["k", "9001"]) lifted.
	for ev := range g.query(nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupRemoveUser}}, loadLimit) {
		if ev.Tags.Has("ban") {
			for t := range ev.Tags.FindAll("p") {
				if pk, err := nostr.PubKeyFromHex(t[1]); err == nil {
					g.ban(groupID(ev), pk, ev.ID)
				}
			}
		}
	}
	for ev := range g.query(nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupDeleteEvent}, Tags: nostr.TagMap{"k": {"9001"}}}, loadLimit) {
		g.unban(groupID(ev), ev)
	}
	return g, nil
}

// Bind gives the relay the group rules and lets groups publish through it.
func (g *Groups) Bind(rl *khatru.Relay) {
	g.relay = rl
	rl.Info.Self = &g.self
	rl.OnEventSaved = g.saved
	rl.PreventBroadcast = g.PreventBroadcast
	// Clients send back the owner and roles they read: give them some.
	g.mu.Lock()
	for _, id := range g.stale {
		g.publish(context.Background(), id)
	}
	g.stale = nil
	g.unlock()
	stored := rl.QueryStored
	rl.QueryStored = func(ctx context.Context, f nostr.Filter) iter.Seq[nostr.Event] {
		readers := khatru.GetAllAuthed(ctx)
		return func(yield func(nostr.Event) bool) {
			// Collected first: Readable takes g.mu, which a writer may hold while
			// bbolt waits for this read transaction to end (deadlock).
			// ponytail: filtered after the store's limit, so a page can come back short.
			for _, ev := range slices.Collect(stored(ctx, f)) {
				if g.Readable(readers, ev) && !yield(ev) {
					return
				}
			}
		}
	}
}

// The bbolt store preallocates `limit` slots: keep it bounded.
// ponytail: past 100k groups (or invites) at startup, page through with `until`.
const loadLimit = 100_000

func (g *Groups) query(f nostr.Filter, limit int) iter.Seq[nostr.Event] {
	return g.store.QueryEvents(f, limit)
}

func (g *Groups) newGroup(id string) *nip29.Group {
	group, _ := nip29.NewGroup(g.relayURL, id)
	group.Address.Self = g.self
	group.Roles = []*nip29.Role{
		{Name: RoleAdmin, Description: "Everything: roles, channels, invites, removals"},
		{Name: RoleModerator, Description: "Delete messages, remove members, create invites"},
	}
	return &group
}

// community is the group holding members and roles: g itself, or its parent.
func (g *Groups) community(id string) *nip29.Group {
	group := g.groups[id]
	if group != nil && group.Parent != "" {
		return g.groups[group.Parent]
	}
	return group
}

// creator: the author of the first 9007 for id.
func (g *Groups) creator(id string) nostr.PubKey {
	var pk nostr.PubKey
	first := nostr.Timestamp(0)
	for ev := range g.query(nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupCreateGroup}, Tags: nostr.TagMap{"h": {id}}}, 10) {
		if first == 0 || ev.CreatedAt < first {
			pk, first = ev.PubKey, ev.CreatedAt
		}
	}
	return pk
}

// layout of group id; an empty one for a group that has none yet.
func (g *Groups) layout(id string) *layout {
	l := g.layouts[id]
	if l == nil {
		l = &layout{In: map[string]string{}}
		g.layouts[id] = l
	}
	return l
}

// policy of community c; the defaults for one that has none yet.
func (g *Groups) policy(c *nip29.Group) *Policy {
	p := g.policies[c.Address.ID]
	if p == nil {
		p = &Policy{Roles: DefaultRoles()}
		g.policies[c.Address.ID] = p
	}
	return p
}

// unban lifts the bans of community that ev (a 9005) names; true if it named one.
func (g *Groups) unban(community string, ev nostr.Event) bool {
	lifted := false
	for t := range ev.Tags.FindAll("e") {
		if id, err := nostr.IDFromHex(t[1]); err == nil {
			maps.DeleteFunc(g.bans[community], func(_ nostr.PubKey, ban nostr.ID) bool {
				lifted = lifted || ban == id
				return ban == id
			})
		}
	}
	return lifted
}

func (g *Groups) ban(community string, pk nostr.PubKey, id nostr.ID) {
	if g.bans[community] == nil {
		g.bans[community] = map[nostr.PubKey]nostr.ID{}
	}
	g.bans[community][pk] = id
}

// timeout sets (until > now) or lifts pk's timeout in community.
func (g *Groups) timeout(community string, pk nostr.PubKey, until nostr.Timestamp) {
	if until <= nostr.Now() {
		delete(g.timeouts[community], pk)
		return
	}
	if g.timeouts[community] == nil {
		g.timeouts[community] = map[nostr.PubKey]nostr.Timestamp{}
	}
	g.timeouts[community][pk] = until
}

// member: pk as the permission rules see it.
func (g *Groups) member(c *nip29.Group, pk nostr.PubKey) Member {
	m := Member{PubKey: pk, Timeout: g.timeouts[c.Address.ID][pk]}
	for _, r := range c.Members[pk] {
		m.Roles = append(m.Roles, r.Name)
	}
	return m
}

// perms: pk's permissions in community c, or in its channel ch (nil: the community).
func (g *Groups) perms(c *nip29.Group, pk nostr.PubKey, ch *nip29.Group) Perms {
	p := g.policy(c)
	if !isMember(c, pk) && pk != p.Owner {
		return 0
	}
	var channel []Override
	if ch != nil && ch != c {
		channel = g.layout(ch.Address.ID).overrides()
	}
	return p.Effective(g.member(c, pk), channel, nostr.Now())
}

// private: a member without roles can't see channel ch of c (spec § 9, Salons privés).
func (g *Groups) private(c, ch *nip29.Group) bool {
	return !g.policy(c).Effective(Member{}, g.layout(ch.Address.ID).overrides(), nostr.Now()).Has("view_channel")
}

// checkOverrides: author may change channel ch's overrides from old to next.
func (g *Groups) checkOverrides(c *nip29.Group, author nostr.PubKey, can Perms, old, next []Override) error {
	changed := changedOverrides(old, next)
	if len(changed) == 0 {
		return nil
	}
	if !can.Has("manage_roles") {
		return errors.New("restricted: channel permissions need manage_roles")
	}
	policy, actor := g.policy(c), g.member(c, author)
	for target, perms := range changed {
		pk, err := nostr.PubKeyFromHex(target)
		switch {
		case perms&^can != 0:
			return fmt.Errorf("restricted: you don't have %v here", (perms &^ can).Names())
		case target == Everyone:
		case err == nil:
			if !policy.Outranks(actor, g.member(c, pk)) {
				return fmt.Errorf("restricted: %s isn't below you", target)
			}
		default:
			if _, ok := policy.role(target); !ok {
				return fmt.Errorf("invalid: unknown role %q", target)
			}
			if !policy.RoleBelow(actor, target) {
				return fmt.Errorf("restricted: role %q isn't below yours", target)
			}
		}
	}
	return nil
}

// symmetricDiff: the ids in exactly one of a and b.
func symmetricDiff(a, b []string) []string {
	var out []string
	for _, x := range a {
		if !slices.Contains(b, x) {
			out = append(out, x)
		}
	}
	for _, x := range b {
		if !slices.Contains(a, x) && !slices.Contains(out, x) {
			out = append(out, x)
		}
	}
	return out
}

func tagValue(ev nostr.Event, name string) string {
	if t := ev.Tags.Find(name); t != nil {
		return t[1]
	}
	return ""
}

func isMember(c *nip29.Group, pk nostr.PubKey) bool {
	_, ok := c.Members[pk]
	return ok
}

func groupID(ev nostr.Event) string {
	if t := ev.Tags.Find("h"); t != nil {
		return t[1]
	}
	return ""
}

func reject(format string, args ...any) (bool, string) {
	return true, fmt.Sprintf(format, args...)
}

// Check decides on a group event before it's stored (spec protocol § 9).
func (g *Groups) Check(ctx context.Context, ev nostr.Event) (bool, string) {
	if slices.Contains(stateKinds, ev.Kind) {
		return reject("restricted: group state is published by the relay")
	}
	foreign := g.rules != nil && !g.dir.IsLocal(ev.PubKey)
	// Where a newcomer from elsewhere comes from: checked before locking (HTTP).
	joining := ""
	if foreign && ev.Kind == nostr.KindSimpleGroupJoinRequest {
		if t := ev.Tags.Find("nip05"); t != nil {
			domain, err := g.verify.Instance(ctx, t[1], ev.PubKey)
			if err != nil {
				return reject("invalid: %v", err)
			}
			joining = domain
		}
		if !g.rules.Get().AllowsInstance(joining) {
			return reject("blocked: members of your instance can't join communities here")
		}
	}
	g.mu.Lock()
	defer g.mu.Unlock()
	if foreign && ev.Kind != nostr.KindSimpleGroupJoinRequest && !g.rules.Get().AllowsInstance(g.origins[ev.PubKey]) {
		return reject("blocked: your instance is blocked here")
	}

	id := groupID(ev)
	group := g.groups[id]
	if ev.Kind == nostr.KindSimpleGroupCreateGroup {
		parent := tagValue(ev, "parent")
		switch {
		case !idRe.MatchString(id):
			return reject("invalid: group id must be 12 characters a-z 0-9")
		case group != nil:
			return reject("duplicate: this group already exists")
		case parent == "" && !g.dir.IsLocal(ev.PubKey):
			return reject("restricted: claim a handle on this instance first")
		case parent == "":
			return false, ""
		}
		// A channel: from anyone with manage_channels, attached by the relay (spec § 9).
		c := g.groups[parent]
		cat := tagValue(ev, "category")
		switch {
		case c == nil || c.Parent != "" || c.Hidden:
			return reject("invalid: unknown community %q", parent)
		case !g.perms(c, ev.PubKey, nil).Has("manage_channels"):
			return reject("restricted: creating channels needs manage_channels")
		case len(c.Children) >= maxChannels:
			return reject("invalid: %d channels at most", maxChannels)
		case cat != "" && !g.layout(parent).has(cat):
			return reject("invalid: unknown category %q", cat)
		}
		if err := checkChannel(ev, nil); err != nil {
			return reject("%s", err)
		}
		if err := g.checkOverrides(c, ev.PubKey, g.perms(c, ev.PubKey, nil), nil, ParseOverrides(ev.Tags)); err != nil {
			return reject("%s", err)
		}
		return false, ""
	}
	if group == nil {
		return reject("invalid: unknown group")
	}
	c := g.community(id)
	if c == nil {
		return reject("invalid: unknown community")
	}
	author := ev.PubKey
	policy := g.policy(c)
	can := g.perms(c, author, group)

	switch ev.Kind {
	case nostr.KindSimpleGroupChatMessage:
		switch {
		case group == c || group.LiveKit:
			return reject("invalid: write in a text channel")
		case !can.Has("send_messages"):
			return reject("restricted: you can't write here")
		}
		if s := g.layout(id).Slowmode; s > 0 && !can.Has("manage_messages") && !can.Has("manage_channels") {
			now, key := nostr.Now(), id+author.Hex()
			if wait := int64(g.spoke[key]) + int64(s) - int64(now); wait > 0 {
				return reject("slow-mode: wait %ds", wait)
			}
			g.spoke[key] = now
		}
	case nostr.KindReaction:
		target, _ := nostr.IDFromHex(tagValue(ev, "e"))
		switch {
		case group == c || group.LiveKit:
			return reject("invalid: react in a text channel")
		case !can.Has("add_reactions"):
			return reject("restricted: you can't react here")
		case ev.Content == "" || len(ev.Content) > maxReaction:
			return reject("invalid: a reaction is 1 to %d bytes", maxReaction)
		case !g.messageIn(id, target):
			return reject("invalid: react to a message of this channel")
		}
		for prev := range g.query(nostr.Filter{Kinds: []nostr.Kind{nostr.KindReaction}, Authors: []nostr.PubKey{author}, Tags: nostr.TagMap{"e": {target.Hex()}}}, 100) {
			if prev.Content == ev.Content {
				return reject("duplicate: already reacted with this")
			}
		}
	case KindSetPins:
		switch {
		case group == c || group.LiveKit:
			return reject("invalid: pin in a text channel")
		case !can.Has("manage_messages"):
			return reject("restricted: pinning needs manage_messages")
		}
		n := 0
		for t := range ev.Tags.FindAll("e") {
			n++
			target, err := nostr.IDFromHex(t[1])
			if err != nil || !g.messageIn(id, target) {
				return reject("invalid: pin messages of this channel")
			}
		}
		if n > maxPins {
			return reject("invalid: %d pins at most", maxPins)
		}
	case nostr.KindSimpleGroupEditMetadata:
		return g.checkEdit(ev, group)
	case nostr.KindSimpleGroupPutUser:
		if t := ev.Tags.Find("timeout"); t != nil {
			return g.checkTimeout(ev, c, group, t[1])
		}
		if group != c || !can.Has("manage_roles") {
			return reject("restricted: giving roles needs manage_roles, on the community")
		}
		actor := g.member(c, author)
		for t := range ev.Tags.FindAll("p") {
			pk, err := nostr.PubKeyFromHex(t[1])
			switch {
			case err != nil:
				return reject("invalid: bad p tag")
			case !isMember(c, pk):
				return reject("invalid: not a member")
			case pk != author && !policy.Outranks(actor, g.member(c, pk)):
				return reject("restricted: they're not below you")
			}
			for _, id := range t[2:] {
				if _, ok := policy.role(id); !ok || id == Everyone {
					return reject("invalid: unknown role %q", id)
				}
			}
			// Roles given or taken: below the actor's own.
			for _, id := range symmetricDiff(g.member(c, pk).Roles, t[2:]) {
				if !policy.RoleBelow(actor, id) {
					return reject("restricted: role %q isn't below yours", id)
				}
			}
		}
	case nostr.KindSimpleGroupRemoveUser:
		banning := ev.Tags.Has("ban")
		switch {
		case group != c:
			return reject("invalid: remove members from the community")
		case banning && !can.Has("ban_members"):
			return reject("restricted: banning needs ban_members")
		case !banning && !can.Has("kick_members"):
			return reject("restricted: removing members needs kick_members")
		case len(ev.Content) > maxReason:
			return reject("invalid: a reason is %d bytes at most", maxReason)
		}
		actor := g.member(c, author)
		for t := range ev.Tags.FindAll("p") {
			pk, err := nostr.PubKeyFromHex(t[1])
			switch {
			case err != nil:
				return reject("invalid: bad p tag")
			case !policy.Outranks(actor, g.member(c, pk)):
				return reject("restricted: they're not below you")
			case banning && g.bans[c.Address.ID][pk] != nostr.ZeroID:
				return reject("duplicate: already banned")
			}
		}
	case nostr.KindSimpleGroupDeleteEvent:
		return g.checkDelete(ev, c)
	case nostr.KindSimpleGroupDeleteGroup:
		if group == c && author != policy.Owner {
			return reject("restricted: only the owner deletes the community")
		}
		if group != c && !can.Has("manage_channels") {
			return reject("restricted: deleting a channel needs manage_channels")
		}
		if group.Hidden {
			return reject("invalid: already deleted")
		}
	case nostr.KindSimpleGroupCreateInvite:
		if group != c || !can.Has("create_invite") {
			return reject("restricted: inviting needs create_invite, on the community")
		}
		code, inv, err := parseInvite(ev)
		switch {
		case err != nil:
			return reject("%s", err)
		case inv.Expires != 0 && inv.Expires <= nostr.Now():
			return reject("invalid: this invite has already expired")
		case g.invites[id][code] != nil:
			return reject("duplicate: this code is taken")
		}
	case nostr.KindSimpleGroupJoinRequest:
		code := ev.Tags.Find("code")
		switch {
		case group != c:
			return reject("invalid: join the community, not a channel")
		case isMember(c, author):
			return reject("duplicate: already a member")
		case g.bans[c.Address.ID][author] != nostr.ZeroID:
			return reject("blocked: you're banned from this community")
		case code == nil || g.invites[id][code[1]] == nil || !g.invites[id][code[1]].usable(nostr.Now()):
			return reject("restricted: invalid invite code (unknown, expired, used up or revoked)")
		}
	case nostr.KindSimpleGroupLeaveRequest:
		switch {
		case group != c || !isMember(c, author):
			return reject("invalid: not a member of this community")
		case author == policy.Owner:
			return reject("restricted: the owner can't leave; transfer the community first")
		}
	}
	return false, ""
}

// checkEdit: a channel is edited with manage_channels in it; it keeps its
// community and type (spec § 9, Salons et catégories). A community's 9002
// carries all its settings; each one changed needs its permission (Réglages).
func (g *Groups) checkEdit(ev nostr.Event, group *nip29.Group) (bool, string) {
	parent := tagValue(ev, "parent")
	switch {
	case group.Hidden:
		return reject("invalid: deleted")
	case group.Parent != "" && parent != group.Parent:
		return reject("invalid: a channel can't change community")
	case group.Parent == "" && parent != "":
		return reject("invalid: a community can't become a channel")
	case group.Parent != "":
		if err := checkChannel(ev, group); err != nil {
			return reject("%s", err)
		}
		c := g.groups[group.Parent]
		can := g.perms(c, ev.PubKey, group)
		old, next := g.layout(group.Address.ID).Overrides, ParseOverrides(ev.Tags)
		changedOther := tagValue(ev, "name") != group.Name || tagValue(ev, "about") != group.About ||
			channelLayout(ev.Tags).Slowmode != g.layout(group.Address.ID).Slowmode ||
			channelLayout(ev.Tags).UserLimit != g.layout(group.Address.ID).UserLimit
		if changedOther && !can.Has("manage_channels") {
			return reject("restricted: editing channels needs manage_channels")
		}
		if !changedOther && len(changedOverrides(old, next)) == 0 && !can.Has("manage_channels") && !can.Has("manage_roles") {
			return reject("restricted: editing channels needs manage_channels")
		}
		if err := g.checkOverrides(c, ev.PubKey, can, old, next); err != nil {
			return reject("%s", err)
		}
	default:
		can := g.perms(group, ev.PubKey, nil)
		if err := checkCategories(ev.Tags); err != nil {
			return reject("%s", err)
		}
		next, cur := parseLayout(ev.Tags), g.layout(group.Address.ID)
		children := arrange(group.Children, childIDs(ev.Tags))
		moved := !slices.Equal(children, group.Children) || !slices.Equal(next.Categories, cur.Categories) ||
			!maps.Equal(next.placed(children), cur.placed(children))
		if moved && !can.Has("manage_channels") {
			return reject("restricted: arranging channels needs manage_channels")
		}
		if tagValue(ev, "name") != group.Name || tagValue(ev, "picture") != group.Picture ||
			tagValue(ev, "about") != group.About || tagValue(ev, "banner") != group.Banner {
			if !can.Has("manage_community") {
				return reject("restricted: editing the community needs manage_community")
			}
		}
		// Without them, an older client's 9002 would wipe the roles: refused.
		owner, err := nostr.PubKeyFromHex(tagValue(ev, "owner"))
		roles := ParseRoles(ev.Tags)
		if err != nil || roles == nil {
			return reject("invalid: a community's 9002 carries its owner and roles; update the app")
		}
		policy := g.policy(group)
		if owner != policy.Owner {
			switch {
			case ev.PubKey != policy.Owner:
				return reject("restricted: only the owner gives the community away")
			case !isMember(group, owner):
				return reject("invalid: the new owner must be a member")
			}
		}
		if !slices.EqualFunc(roles, policy.Roles, Role.equal) {
			if err := policy.CheckRoles(g.member(group, ev.PubKey), roles); err != nil {
				return reject("%s", err)
			}
		}
	}
	return false, ""
}

// checkDelete (spec § 9, Modération): a message by its author or with
// manage_messages in its channel; a ban with ban_members; nothing else.
func (g *Groups) checkDelete(ev nostr.Event, c *nip29.Group) (bool, string) {
	for t := range ev.Tags.FindAll("e") {
		id, err := nostr.IDFromHex(t[1])
		if err != nil {
			return reject("invalid: bad e tag")
		}
		for target := range g.query(nostr.Filter{IDs: []nostr.ID{id}}, 1) {
			switch {
			case g.community(groupID(target)) != c:
				return reject("restricted: not in this community")
			case target.Kind == nostr.KindSimpleGroupChatMessage || target.Kind == nostr.KindReaction:
				if target.PubKey != ev.PubKey && !g.perms(c, ev.PubKey, g.groups[groupID(target)]).Has("manage_messages") {
					return reject("restricted: you can only delete your own")
				}
			case target.Kind == nostr.KindSimpleGroupRemoveUser && target.Tags.Has("ban"):
				if !g.perms(c, ev.PubKey, nil).Has("ban_members") {
					return reject("restricted: unbanning needs ban_members")
				}
				if tagValue(ev, "p") != tagValue(target, "p") {
					return reject("invalid: an unban names who it lets back (p)")
				}
			case target.Kind == nostr.KindSimpleGroupCreateInvite:
				if target.PubKey != ev.PubKey && !g.perms(c, ev.PubKey, nil).Has("manage_invites") {
					return reject("restricted: revoking others' invites needs manage_invites")
				}
			default:
				return reject("restricted: the audit log can't be erased")
			}
		}
	}
	return false, ""
}

// checkTimeout: a 9000 ["timeout", until] (spec § 9, Modération). g.mu held.
func (g *Groups) checkTimeout(ev nostr.Event, c, group *nip29.Group, until string) (bool, string) {
	t, err := strconv.ParseInt(until, 10, 64)
	switch {
	case group != c:
		return reject("invalid: time out on the community")
	case !g.perms(c, ev.PubKey, nil).Has("moderate_members"):
		return reject("restricted: timeouts need moderate_members")
	case err != nil || t < 0 || t > int64(nostr.Now())+maxTimeout:
		return reject("invalid: a timeout ends within 28 days (0 lifts it)")
	case len(ev.Content) > maxReason:
		return reject("invalid: a reason is %d bytes at most", maxReason)
	}
	actor := g.member(c, ev.PubKey)
	for p := range ev.Tags.FindAll("p") {
		pk, err := nostr.PubKeyFromHex(p[1])
		switch {
		case err != nil:
			return reject("invalid: bad p tag")
		case len(p) > 2:
			return reject("invalid: a timeout doesn't give roles")
		case !isMember(c, pk):
			return reject("invalid: not a member")
		case !g.policy(c).Outranks(actor, g.member(c, pk)):
			return reject("restricted: they're not below you")
		}
	}
	return false, ""
}

// CheckFilter: group events (messages, moderation) are only served to members,
// and only with an explicit #h. Group state (39000+) stays public.
func (g *Groups) CheckFilter(ctx context.Context, f nostr.Filter) (bool, string) {
	// A private channel's state, asked by #d: authenticate first, or it's filtered out unseen.
	if len(khatru.GetAllAuthed(ctx)) == 0 && g.asksPrivate(f) {
		return reject("auth-required: private channels are for those who may see them")
	}
	touches := len(f.Tags["h"]) > 0 || len(f.Kinds) == 0
	for _, k := range f.Kinds {
		touches = touches || slices.Contains(ClientKinds, k)
	}
	if !touches {
		return false, ""
	}
	if len(f.Tags["h"]) == 0 { // also a filter without kinds: it could match group events
		return reject("restricted: ask for group events with kinds and #h")
	}
	authed := khatru.GetAllAuthed(ctx)
	if len(authed) == 0 {
		return reject("auth-required: communities are for their members")
	}
	g.mu.Lock()
	defer g.mu.Unlock()
	for _, id := range f.Tags["h"] {
		c := g.community(id)
		if c == nil || !slices.ContainsFunc(authed, func(pk nostr.PubKey) bool { return isMember(c, pk) }) {
			return reject("restricted: you're not a member of %q", id)
		}
	}
	return false, ""
}

func (g *Groups) asksPrivate(f nostr.Filter) bool {
	g.mu.Lock()
	defer g.mu.Unlock()
	for _, id := range f.Tags["d"] {
		if !g.seesChannel(nil, id) {
			return true
		}
	}
	return false
}

// PreventBroadcast applies Readable to live events too: CheckFilter only runs
// when the subscription opens, and a member can lose a permission or leave since.
func (g *Groups) PreventBroadcast(ws *khatru.WebSocket, _ nostr.Filter, ev nostr.Event) bool {
	return !g.Readable(ws.AuthedPublicKeys, ev)
}

// Readable: one of readers may read ev (spec § 9, Lecture). Anything but
// requests from clients is left to the other policies.
func (g *Groups) Readable(readers []nostr.PubKey, ev nostr.Event) bool {
	if ev.Kind == nostr.KindSimpleGroupMetadata || ev.Kind == nostr.KindSimpleGroupLiveKitParticipants || ev.Kind == KindPins {
		g.mu.Lock()
		defer g.mu.Unlock()
		return g.seesChannel(readers, ev.Tags.GetD())
	}
	if !slices.Contains(ClientKinds, ev.Kind) {
		return true
	}
	g.mu.Lock()
	defer g.mu.Unlock()
	c := g.community(groupID(ev))
	if c == nil {
		return false
	}
	for _, pk := range readers {
		if pk == ev.PubKey {
			return true
		}
		switch ev.Kind {
		case nostr.KindSimpleGroupChatMessage, nostr.KindReaction, nostr.KindSimpleGroupDeleteEvent:
			if k := tagValue(ev, "k"); ev.Kind == nostr.KindSimpleGroupDeleteEvent && (k == "9001" || k == "9009") {
				if can := g.perms(c, pk, nil); can.Has("view_audit_log") || (k == "9009" && can.Has("manage_invites")) {
					return true
				}
			} else if g.perms(c, pk, g.groups[groupID(ev)]).Has("view_channel") {
				return true
			}
		case nostr.KindSimpleGroupCreateInvite, nostr.KindSimpleGroupJoinRequest:
			if can := g.perms(c, pk, nil); can.Has("view_audit_log") || can.Has("manage_invites") {
				return true
			}
		default:
			if g.perms(c, pk, nil).Has("view_audit_log") {
				return true
			}
		}
	}
	return false
}

// messageIn: id is a message (9) of channel. g.mu held.
func (g *Groups) messageIn(channel string, id nostr.ID) bool {
	for ev := range g.query(nostr.Filter{IDs: []nostr.ID{id}}, 1) {
		return ev.Kind == nostr.KindSimpleGroupChatMessage && groupID(ev) == channel
	}
	return false
}

// seesChannel: group id isn't a private channel, or one of readers has
// view_channel in it (spec § 9, Salons privés). g.mu held.
func (g *Groups) seesChannel(readers []nostr.PubKey, id string) bool {
	ch := g.groups[id]
	if ch == nil || ch.Parent == "" {
		return true
	}
	c := g.groups[ch.Parent]
	if c == nil || !g.private(c, ch) {
		return true
	}
	return slices.ContainsFunc(readers, func(pk nostr.PubKey) bool { return g.perms(c, pk, ch).Has("view_channel") })
}

// saved applies an accepted group event and publishes the new state.
func (g *Groups) saved(ctx context.Context, ev nostr.Event) {
	if !slices.Contains(ClientKinds, ev.Kind) || ev.Kind == nostr.KindSimpleGroupChatMessage || ev.Kind == nostr.KindReaction {
		return
	}
	if ev.Kind == KindSetPins {
		g.mu.Lock()
		defer g.unlock()
		pins := nostr.Event{Kind: KindPins, Tags: nostr.Tags{{"d", groupID(ev)}}}
		for t := range ev.Tags.FindAll("e") {
			pins.Tags = append(pins.Tags, nostr.Tag{"e", t[1]})
		}
		g.emit(ctx, groupID(ev), pins)
		return
	}
	g.mu.Lock()
	defer g.unlock()

	id := groupID(ev)
	switch ev.Kind {
	case nostr.KindSimpleGroupCreateGroup:
		g.groups[id] = g.newGroup(id)
		if parent := tagValue(ev, "parent"); parent != "" {
			g.createChannel(ctx, ev, parent)
			return
		}
		g.policies[id] = &Policy{Owner: ev.PubKey, Roles: DefaultRoles()}
		g.put(ctx, id, ev.PubKey, RoleAdmin)
		g.publish(ctx, id)
		return
	case nostr.KindSimpleGroupJoinRequest:
		if t := ev.Tags.Find("nip05"); t != nil && !g.dir.IsLocal(ev.PubKey) {
			if _, domain, ok := strings.Cut(t[1], "@"); ok {
				g.origins[ev.PubKey] = rules.Domain(domain)
			}
		}
		if inv := g.invites[id][tagValue(ev, "code")]; inv != nil {
			inv.Uses++
		}
		g.put(ctx, id, ev.PubKey)
		g.publish(ctx, id)
		return
	case nostr.KindSimpleGroupLeaveRequest:
		g.relayEvent(ctx, nostr.KindSimpleGroupRemoveUser, id, nostr.Tag{"p", ev.PubKey.Hex()})
		delete(g.groups[id].Members, ev.PubKey)
		g.publish(ctx, id)
		return
	case nostr.KindSimpleGroupDeleteEvent:
		// An unban lifts the ban and keeps it for the log; a message goes.
		if g.unban(id, ev) {
			return
		}
		for t := range ev.Tags.FindAll("e") {
			if target, err := nostr.IDFromHex(t[1]); err == nil {
				g.revokeInvite(id, target)
				_ = g.relay.DeleteEvent(ctx, target)
			}
		}
		return
	case nostr.KindSimpleGroupCreateInvite:
		g.addInvite(ev) // codes stay secret: no state to publish
		return
	case nostr.KindSimpleGroupPutUser:
		if t := ev.Tags.Find("timeout"); t != nil {
			until, _ := strconv.ParseInt(t[1], 10, 64)
			for p := range ev.Tags.FindAll("p") {
				if pk, err := nostr.PubKeyFromHex(p[1]); err == nil {
					g.timeout(id, pk, nostr.Timestamp(until))
				}
			}
			g.publish(ctx, id)
			return
		}
	case nostr.KindSimpleGroupRemoveUser:
		for p := range ev.Tags.FindAll("p") {
			if pk, err := nostr.PubKeyFromHex(p[1]); err == nil {
				delete(g.timeouts[id], pk)
				if ev.Tags.Has("ban") {
					g.ban(id, pk, ev.ID)
				}
			}
		}
	}

	group := g.groups[id]
	action, err := nip29.PrepareModerationAction(ev)
	if group == nil || err != nil {
		g.relay.Log.Printf("community: %s on %s: %v", ev.Kind, id, err)
		return
	}
	before := group.Children
	action.Apply(group)
	switch {
	case ev.Kind == nostr.KindSimpleGroupEditMetadata && group.Parent != "":
		group.Children = nil
		g.layouts[id] = channelLayout(ev.Tags)
	case ev.Kind == nostr.KindSimpleGroupEditMetadata:
		group.Children = arrange(before, group.Children)
		l := parseLayout(ev.Tags)
		g.layouts[id] = &layout{Categories: l.Categories, In: l.placed(group.Children)}
		g.applySettings(group, ev)
	case ev.Kind == nostr.KindSimpleGroupDeleteGroup && group.Parent != "":
		// Its community lists it no more (spec § 9).
		if c := g.groups[group.Parent]; c != nil {
			c.Children = slices.DeleteFunc(slices.Clone(c.Children), func(ch string) bool { return ch == id })
			delete(g.layout(c.Address.ID).In, id)
			g.publish(ctx, c.Address.ID)
		}
	case ev.Kind == nostr.KindSimpleGroupDeleteGroup && group.Parent == "":
		// A deleted community takes its channels and invites with it.
		delete(g.invites, id)
		for _, child := range group.Children {
			if ch := g.groups[child]; ch != nil {
				action.Apply(ch)
				g.publish(ctx, child)
			}
		}
	}
	g.publish(ctx, id)
}

// createChannel sets up channel id from its 9007 and adds it at the end of
// its community (spec § 9, Salons et catégories).
func (g *Groups) createChannel(ctx context.Context, ev nostr.Event, parent string) {
	id := groupID(ev)
	ch, c := g.groups[id], g.groups[parent]
	ch.Parent, ch.Name, ch.About = parent, tagValue(ev, "name"), tagValue(ev, "about")
	ch.LiveKit = ev.Tags.Has("livekit")
	g.layouts[id] = channelLayout(ev.Tags)
	c.Children = append(slices.Clone(c.Children), id)
	if cat := tagValue(ev, "category"); cat != "" {
		g.layout(parent).In[id] = cat
	}
	g.publish(ctx, id)
	g.publish(ctx, parent)
}

// applySettings keeps the owner and roles of an accepted community 9002 (the
// library drops those tags); members lose the roles that are gone.
func (g *Groups) applySettings(c *nip29.Group, ev nostr.Event) {
	p := g.policy(c)
	if owner, err := nostr.PubKeyFromHex(tagValue(ev, "owner")); err == nil {
		p.Owner = owner
	}
	if roles := ParseRoles(ev.Tags); roles != nil {
		p.Roles = roles
	}
	for pk, roles := range c.Members {
		c.Members[pk] = slices.DeleteFunc(roles, func(r *nip29.Role) bool {
			_, ok := p.role(r.Name)
			return !ok
		})
	}
}

// put makes pk a member (with roles) through a relay-signed 9000, kept for the record.
func (g *Groups) put(ctx context.Context, id string, pk nostr.PubKey, roles ...string) {
	ev := g.relayEvent(ctx, nostr.KindSimpleGroupPutUser, id, append(nostr.Tag{"p", pk.Hex()}, roles...))
	if action, err := nip29.PrepareModerationAction(ev); err == nil {
		action.Apply(g.groups[id])
	}
}

func (g *Groups) relayEvent(_ context.Context, kind nostr.Kind, id string, tags ...nostr.Tag) nostr.Event {
	ev := nostr.Event{
		Kind:      kind,
		CreatedAt: nostr.Now(),
		Tags:      append(nostr.Tags{{"v", "0"}, {"h", id}}, tags...),
	}
	g.sign(&ev)
	if err := g.store.SaveEvent(ev); err != nil {
		g.relay.Log.Printf("community: save %s: %v", ev.Kind, err)
	}
	g.outbox = append(g.outbox, ev)
	return ev
}

// publish signs, stores and broadcasts the state of group id. Communities are
// always private, closed and restricted (spec § 9); channels only get 39000.
func (g *Groups) publish(ctx context.Context, id string) {
	group := g.groups[id]
	group.Private, group.Closed, group.Restricted = true, true, true
	meta := group.ToMetadataEvent()
	g.layout(id).apply(&meta)
	if group.Parent != "" {
		g.emit(ctx, id, meta)
		return
	}
	p := g.policy(group)
	meta.Tags = append(meta.Tags, nostr.Tag{"owner", p.Owner.Hex()})
	roles := nostr.Event{Kind: nostr.KindSimpleGroupRoles, Tags: append(nostr.Tags{{"d", id}}, RoleTags(p.Roles)...)}
	members := group.ToMembersEvent()
	for _, pk := range slices.SortedFunc(maps.Keys(g.timeouts[id]), func(a, b nostr.PubKey) int { return strings.Compare(a.Hex(), b.Hex()) }) {
		if until := g.timeouts[id][pk]; until > nostr.Now() && isMember(group, pk) {
			members.Tags = append(members.Tags, nostr.Tag{"timeout", pk.Hex(), strconv.FormatInt(int64(until), 10)})
		}
	}
	g.emit(ctx, id, meta, group.ToAdminsEvent(), members, roles)
}

// emit signs, stores and broadcasts state events of group id (g.mu held).
func (g *Groups) emit(ctx context.Context, id string, events ...nostr.Event) {
	stamp := max(nostr.Now(), g.stamps[id]+1)
	g.stamps[id] = stamp
	for _, ev := range events {
		ev.CreatedAt = stamp
		ev.Tags = append(nostr.Tags{{"v", "0"}}, ev.Tags...)
		g.sign(&ev)
		if err := g.relay.ReplaceEvent(ctx, ev); err != nil {
			g.relay.Log.Printf("community: replace %s: %v", ev.Kind, err)
			continue
		}
		g.outbox = append(g.outbox, ev)
	}
}

// Voice: channel id as a voice channel (spec § 11), with pk's permissions in it (§ 9); false if it isn't one.
func (g *Groups) Voice(id string, pk nostr.PubKey) (VoiceChannel, bool) {
	g.mu.Lock()
	defer g.mu.Unlock()
	group := g.groups[id]
	c := g.community(id)
	if group == nil || !group.LiveKit || group.Parent == "" || c == nil {
		return VoiceChannel{}, false
	}
	return VoiceChannel{Community: c.Address.ID, Perms: g.perms(c, pk, group), UserLimit: g.layout(id).UserLimit}, true
}

// VoiceChannel is what voice needs of a voice channel (spec § 11).
type VoiceChannel struct {
	Community string
	Perms     Perms // the key's, in this channel
	UserLimit int   // 0: none
}

// VoiceChannels lists the voice channels of a community, or of every community when it's "".
func (g *Groups) VoiceChannels(community string) []string {
	g.mu.Lock()
	defer g.mu.Unlock()
	var ids []string
	for id, group := range g.groups {
		if group.LiveKit && group.Parent != "" && (community == "" || group.Parent == community) {
			ids = append(ids, id)
		}
	}
	return ids
}

// SetVoiceParticipants publishes who is in voice channel id, and who of them
// moderators muted or deafened (39004, signed by the relay; spec § 11).
func (g *Groups) SetVoiceParticipants(ctx context.Context, id string, pks, muted, deafened []nostr.PubKey) {
	g.mu.Lock()
	defer g.unlock()
	group := g.groups[id]
	if group == nil || !group.LiveKit {
		return
	}
	group.LiveKitParticipants = pks
	ev := group.ToLiveKitParticipantsEvent()
	for _, pk := range muted {
		ev.Tags = append(ev.Tags, nostr.Tag{"muted", pk.Hex()})
	}
	for _, pk := range deafened {
		ev.Tags = append(ev.Tags, nostr.Tag{"deafened", pk.Hex()})
	}
	g.emit(ctx, id, ev)
}

func (g *Groups) sign(ev *nostr.Event) {
	if err := ev.Sign(g.key); err != nil {
		panic(err) // only fails on an invalid key, checked at startup
	}
}

// LoadKey returns the relay key: hexKey (SERVER_RELAYKEY) if set, else the one
// in path, created on first start (0600). Losing it makes clients distrust the
// group state: back it up with the database.
func LoadKey(hexKey, path string) (nostr.SecretKey, error) {
	if hexKey != "" {
		return nostr.SecretKeyFromHex(hexKey)
	}
	raw, err := os.ReadFile(path)
	if err == nil {
		return nostr.SecretKeyFromHex(strings.TrimSpace(string(raw)))
	}
	if !errors.Is(err, os.ErrNotExist) {
		return nostr.SecretKey{}, fmt.Errorf("relay key: %w", err)
	}
	key := nostr.Generate()
	if err := os.WriteFile(path, []byte(key.Hex()+"\n"), 0o600); err != nil {
		return nostr.SecretKey{}, fmt.Errorf("relay key: %w", err)
	}
	return key, nil
}
