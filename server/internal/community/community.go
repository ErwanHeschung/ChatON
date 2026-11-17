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
	"os"
	"regexp"
	"slices"
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

// Kinds a client may publish; 39000-39003 only come from the relay.
var ClientKinds = []nostr.Kind{
	nostr.KindSimpleGroupChatMessage,
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
				if t := ev.Tags.Find("owner"); t != nil {
					owners[id], _ = nostr.PubKeyFromHex(t[1])
				}
			case nostr.KindSimpleGroupAdmins:
				err = group.MergeInAdminsEvent(&ev)
			case nostr.KindSimpleGroupMembers:
				err = group.MergeInMembersEvent(&ev)
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
		if p.Roles == nil {
			p.Roles = defaultRoles()
		}
		if p.Owner == (nostr.PubKey{}) {
			p.Owner = g.creator(id)
		}
		g.policies[id] = p
	}
	// Joins were verified when accepted: their nip05 says where members come from.
	for ev := range g.query(nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupJoinRequest}}, loadLimit) {
		if t := ev.Tags.Find("nip05"); t != nil {
			if _, domain, ok := strings.Cut(t[1], "@"); ok {
				g.origins[ev.PubKey] = rules.Domain(domain)
			}
		}
	}
	for ev := range g.query(nostr.Filter{Kinds: []nostr.Kind{nostr.KindSimpleGroupCreateInvite}}, loadLimit) {
		if group := g.groups[groupID(ev)]; group != nil {
			if action, err := nip29.PrepareModerationAction(ev); err == nil {
				action.Apply(group)
			}
		}
	}
	return g, nil
}

// Bind gives the relay the group rules and lets groups publish through it.
func (g *Groups) Bind(rl *khatru.Relay) {
	g.relay = rl
	rl.Info.Self = &g.self
	rl.OnEventSaved = g.saved
	rl.PreventBroadcast = g.PreventBroadcast
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

// policy of community c; the defaults for one that has none yet.
func (g *Groups) policy(c *nip29.Group) *Policy {
	p := g.policies[c.Address.ID]
	if p == nil {
		p = &Policy{Roles: defaultRoles()}
		g.policies[c.Address.ID] = p
	}
	return p
}

// member: pk as the permission rules see it.
func member(c *nip29.Group, pk nostr.PubKey) Member {
	m := Member{PubKey: pk}
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
		channel = []Override{} // ponytail: channel overrides come with private channels (step 12e)
	}
	return p.Effective(member(c, pk), channel, nostr.Now())
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
		switch {
		case !idRe.MatchString(id):
			return reject("invalid: group id must be 12 characters a-z 0-9")
		case group != nil:
			return reject("duplicate: this group already exists")
		case !g.dir.IsLocal(ev.PubKey):
			return reject("restricted: claim a handle on this instance first")
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
		if !can.Has("send_messages") {
			return reject("restricted: you can't write here")
		}
	case nostr.KindSimpleGroupEditMetadata:
		return g.checkEdit(ev, group)
	case nostr.KindSimpleGroupPutUser:
		if group != c || !can.Has("manage_roles") {
			return reject("restricted: giving roles needs manage_roles, on the community")
		}
		actor := member(c, author)
		for t := range ev.Tags.FindAll("p") {
			pk, err := nostr.PubKeyFromHex(t[1])
			switch {
			case err != nil:
				return reject("invalid: bad p tag")
			case !isMember(c, pk):
				return reject("invalid: not a member")
			case pk != author && !policy.Outranks(actor, member(c, pk)):
				return reject("restricted: they're not below you")
			}
			for _, id := range t[2:] {
				if _, ok := policy.role(id); !ok || id == Everyone {
					return reject("invalid: unknown role %q", id)
				}
			}
			// Roles given or taken: below the actor's own.
			for _, id := range symmetricDiff(member(c, pk).Roles, t[2:]) {
				if !policy.RoleBelow(actor, id) {
					return reject("restricted: role %q isn't below yours", id)
				}
			}
		}
	case nostr.KindSimpleGroupRemoveUser:
		if group != c || !can.Has("kick_members") {
			return reject("restricted: removing members needs kick_members, on the community")
		}
		actor := member(c, author)
		for t := range ev.Tags.FindAll("p") {
			pk, err := nostr.PubKeyFromHex(t[1])
			if err != nil {
				return reject("invalid: bad p tag")
			}
			if !policy.Outranks(actor, member(c, pk)) {
				return reject("restricted: they're not below you")
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
	case nostr.KindSimpleGroupCreateInvite:
		if group != c || !can.Has("create_invite") {
			return reject("restricted: inviting needs create_invite, on the community")
		}
	case nostr.KindSimpleGroupJoinRequest:
		code := ev.Tags.Find("code")
		switch {
		case group != c:
			return reject("invalid: join the community, not a channel")
		case isMember(c, author):
			return reject("duplicate: already a member")
		case code == nil || !slices.Contains(c.InviteCodes, code[1]):
			return reject("restricted: invalid invite code")
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

// checkEdit: a channel is edited with manage_channels in its community, which it
// gets once (spec § 9, channel creation); a community's name and picture need
// manage_community, its channel list manage_channels.
func (g *Groups) checkEdit(ev nostr.Event, group *nip29.Group) (bool, string) {
	parent := ""
	if t := ev.Tags.Find("parent"); t != nil {
		parent = t[1]
	}
	switch {
	case group.Parent != "" && parent != group.Parent:
		return reject("invalid: a channel can't change community")
	case parent != "" && len(group.Children) > 0:
		return reject("invalid: a community can't become a channel")
	case parent != "":
		p := g.groups[parent]
		if p == nil || p.Parent != "" {
			return reject("invalid: unknown community %q", parent)
		}
		if !g.perms(p, ev.PubKey, nil).Has("manage_channels") {
			return reject("restricted: editing channels needs manage_channels")
		}
	default:
		can := g.perms(group, ev.PubKey, nil)
		var children []string
		for t := range ev.Tags.FindAll("child") {
			if child := g.groups[t[1]]; child == nil || child.Parent != group.Address.ID {
				return reject("invalid: %q is not a channel of this community", t[1])
			}
			children = append(children, t[1])
		}
		if !slices.Equal(children, group.Children) && !can.Has("manage_channels") {
			return reject("restricted: arranging channels needs manage_channels")
		}
		if tagValue(ev, "name") != group.Name || tagValue(ev, "picture") != group.Picture ||
			tagValue(ev, "about") != group.About || tagValue(ev, "banner") != group.Banner {
			if !can.Has("manage_community") {
				return reject("restricted: editing the community needs manage_community")
			}
		}
	}
	return false, ""
}

// checkDelete: anyone deletes their own messages; manage_messages in the
// target's channel deletes the others'.
func (g *Groups) checkDelete(ev nostr.Event, c *nip29.Group) (bool, string) {
	for t := range ev.Tags.FindAll("e") {
		id, err := nostr.IDFromHex(t[1])
		if err != nil {
			return reject("invalid: bad e tag")
		}
		for target := range g.query(nostr.Filter{IDs: []nostr.ID{id}}, 1) {
			if g.community(groupID(target)) != c {
				return reject("restricted: not in this community")
			}
			if target.PubKey != ev.PubKey && !g.perms(c, ev.PubKey, g.groups[groupID(target)]).Has("manage_messages") {
				return reject("restricted: you can only delete your own messages")
			}
		}
	}
	return false, ""
}

// CheckFilter: group events (messages, moderation) are only served to members,
// and only with an explicit #h. Group state (39000+) stays public.
func (g *Groups) CheckFilter(ctx context.Context, f nostr.Filter) (bool, string) {
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

// PreventBroadcast stops live group events to connections that are no longer
// members: CheckFilter only runs when the subscription opens.
func (g *Groups) PreventBroadcast(ws *khatru.WebSocket, _ nostr.Filter, ev nostr.Event) bool {
	if !slices.Contains(ClientKinds, ev.Kind) {
		return false
	}
	g.mu.Lock()
	defer g.mu.Unlock()
	c := g.community(groupID(ev))
	return c == nil || !slices.ContainsFunc(ws.AuthedPublicKeys, func(pk nostr.PubKey) bool { return isMember(c, pk) })
}

// saved applies an accepted group event and publishes the new state.
func (g *Groups) saved(ctx context.Context, ev nostr.Event) {
	if !slices.Contains(ClientKinds, ev.Kind) || ev.Kind == nostr.KindSimpleGroupChatMessage {
		return
	}
	g.mu.Lock()
	defer g.mu.Unlock()

	id := groupID(ev)
	switch ev.Kind {
	case nostr.KindSimpleGroupCreateGroup:
		g.groups[id] = g.newGroup(id)
		g.policies[id] = &Policy{Owner: ev.PubKey, Roles: defaultRoles()}
		g.put(ctx, id, ev.PubKey, RoleAdmin)
		g.publish(ctx, id)
		return
	case nostr.KindSimpleGroupJoinRequest:
		if t := ev.Tags.Find("nip05"); t != nil && !g.dir.IsLocal(ev.PubKey) {
			if _, domain, ok := strings.Cut(t[1], "@"); ok {
				g.origins[ev.PubKey] = rules.Domain(domain)
			}
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
		for t := range ev.Tags.FindAll("e") {
			if target, err := nostr.IDFromHex(t[1]); err == nil {
				_ = g.relay.DeleteEvent(ctx, target)
			}
		}
		return
	}

	group := g.groups[id]
	action, err := nip29.PrepareModerationAction(ev)
	if group == nil || err != nil {
		g.relay.Log.Printf("community: %s on %s: %v", ev.Kind, id, err)
		return
	}
	action.Apply(group)
	if ev.Kind == nostr.KindSimpleGroupEditMetadata && group.Parent != "" {
		// ponytail: a channel only keeps name and parent; topics, pictures later.
		group.Children = nil
		delete(g.policies, id) // created as a group of its own, it's a channel now
	}
	if ev.Kind != nostr.KindSimpleGroupCreateInvite { // codes stay secret
		g.publish(ctx, id)
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
	g.relay.BroadcastEvent(ev)
	return ev
}

// publish signs, stores and broadcasts the state of group id. Communities are
// always private, closed and restricted (spec § 9); channels only get 39000.
func (g *Groups) publish(ctx context.Context, id string) {
	group := g.groups[id]
	group.Private, group.Closed, group.Restricted = true, true, true
	meta := group.ToMetadataEvent()
	if group.Parent != "" {
		g.emit(ctx, id, meta)
		return
	}
	p := g.policy(group)
	meta.Tags = append(meta.Tags, nostr.Tag{"owner", p.Owner.Hex()})
	roles := nostr.Event{Kind: nostr.KindSimpleGroupRoles, Tags: append(nostr.Tags{{"d", id}}, RoleTags(p.Roles)...)}
	g.emit(ctx, id, meta, group.ToAdminsEvent(), group.ToMembersEvent(), roles)
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
		g.relay.BroadcastEvent(ev)
	}
}

// VoiceAccess tells whether id is a voice channel (spec § 11) and pk a member of its community.
func (g *Groups) VoiceAccess(id string, pk nostr.PubKey) (voice, member bool) {
	g.mu.Lock()
	defer g.mu.Unlock()
	group := g.groups[id]
	if group == nil || !group.LiveKit || group.Parent == "" {
		return false, false
	}
	c := g.community(id)
	return true, c != nil && isMember(c, pk)
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

// SetVoiceParticipants publishes who is in voice channel id (39004, signed by the relay).
func (g *Groups) SetVoiceParticipants(ctx context.Context, id string, pks []nostr.PubKey) {
	g.mu.Lock()
	defer g.mu.Unlock()
	group := g.groups[id]
	if group == nil || !group.LiveKit {
		return
	}
	group.LiveKitParticipants = pks
	g.emit(ctx, id, group.ToLiveKitParticipantsEvent())
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
