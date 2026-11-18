package community

import (
	"errors"
	"strconv"

	"fiatjaf.com/nostr"
)

// maxInviteUses: the most a 9009 ["max_uses"] allows (spec § 9, Invitations).
const maxInviteUses = 1000

// invite is a community's 9009 as the relay counts it.
type invite struct {
	ID      nostr.ID
	Expires nostr.Timestamp // 0: never
	MaxUses int             // 0: no limit
	Uses    int
}

func (i *invite) usable(now nostr.Timestamp) bool {
	return (i.Expires == 0 || now < i.Expires) && (i.MaxUses == 0 || i.Uses < i.MaxUses)
}

// parseInvite reads a 9009's code and limits.
func parseInvite(ev nostr.Event) (string, *invite, error) {
	code := tagValue(ev, "code")
	if code == "" || len(code) > 32 {
		return "", nil, errors.New("invalid: an invite has a code of 1 to 32 characters")
	}
	inv := &invite{ID: ev.ID}
	if t := ev.Tags.Find("expiration"); t != nil {
		n, err := strconv.ParseInt(t[1], 10, 64)
		if err != nil || n <= 0 {
			return "", nil, errors.New("invalid: bad expiration")
		}
		inv.Expires = nostr.Timestamp(n)
	}
	if t := ev.Tags.Find("max_uses"); t != nil {
		n, err := strconv.Atoi(t[1])
		if err != nil || n < 1 || n > maxInviteUses {
			return "", nil, errors.New("invalid: max_uses is 1 to 1000")
		}
		inv.MaxUses = n
	}
	return code, inv, nil
}

// addInvite records an accepted 9009. g.mu held (or loading).
func (g *Groups) addInvite(ev nostr.Event) {
	code, inv, err := parseInvite(ev)
	if err != nil {
		return
	}
	id := groupID(ev)
	if g.invites[id] == nil {
		g.invites[id] = map[string]*invite{}
	}
	g.invites[id][code] = inv
}

// revokeInvite forgets the invite of community whose 9009 is target; true if there was one.
func (g *Groups) revokeInvite(community string, target nostr.ID) bool {
	for code, inv := range g.invites[community] {
		if inv.ID == target {
			delete(g.invites[community], code)
			return true
		}
	}
	return false
}
