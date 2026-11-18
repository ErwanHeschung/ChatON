// Package presence keeps who is online (spec protocol § 12): ephemeral 24136
// events, in memory only, the last one per key and connection, for 150 s.
package presence

import (
	"context"
	"slices"
	"sync"
	"time"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/khatru"
)

// Kind is a presence signal (ChatON's own, ephemeral).
const Kind nostr.Kind = 24136

// TTL: a signal older than this is gone (devices send one every 60 s).
const TTL = 150 * time.Second

// offline: a device leaving (invisible, a new presence key) replaces its last signal.
var statuses = []string{"online", "idle", "dnd", "offline"}

type entry struct {
	ev nostr.Event
	at time.Time
}

// Store is the relay's memory of presence signals.
type Store struct {
	mu    sync.Mutex
	byKey map[nostr.PubKey]map[*khatru.WebSocket]entry
	// Now is the clock; tests move it.
	Now func() time.Time
}

func New() *Store {
	return &Store{byKey: map[nostr.PubKey]map[*khatru.WebSocket]entry{}, Now: time.Now}
}

// Check is the write policy of a 24136. Without `h`, from a connection
// authenticated as a local user (the key is a presence key); with `h`, from a
// member of that community, authenticated as the event's author.
func (s *Store) Check(ctx context.Context, ev nostr.Event, isLocal func(nostr.PubKey) bool, isMember func(string, nostr.PubKey) bool) (bool, string) {
	status := ev.Tags.Find("status")
	switch {
	case status == nil || !slices.Contains(statuses, status[1]):
		return true, "invalid: status is online, idle, dnd or offline"
	case ev.Content != "":
		return true, "invalid: presence has no content"
	case s.Now().Sub(ev.CreatedAt.Time()) > TTL:
		return true, "invalid: presence too old"
	}
	if h := ev.Tags.Find("h"); h != nil {
		if !khatru.IsAuthed(ctx, ev.PubKey) {
			return true, "auth-required: publish your community presence as yourself"
		}
		if !isMember(h[1], ev.PubKey) {
			return true, "restricted: not a member of this community"
		}
		return false, ""
	}
	if !slices.ContainsFunc(khatru.GetAllAuthed(ctx), isLocal) {
		return true, "auth-required: presence is for this instance's users"
	}
	return false, ""
}

// Put keeps ev as its key's last signal from this connection (OnEphemeralEvent).
func (s *Store) Put(ctx context.Context, ev nostr.Event) {
	s.put(khatru.GetConnection(ctx), ev)
}

func (s *Store) put(conn *khatru.WebSocket, ev nostr.Event) {
	if ev.Kind != Kind || conn == nil {
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.byKey[ev.PubKey] == nil {
		s.byKey[ev.PubKey] = map[*khatru.WebSocket]entry{}
	}
	s.byKey[ev.PubKey][conn] = entry{ev, s.Now()}
}

// Drop forgets what a closing connection sent (OnDisconnect).
func (s *Store) Drop(ctx context.Context) {
	s.drop(khatru.GetConnection(ctx))
}

func (s *Store) drop(conn *khatru.WebSocket) {
	s.mu.Lock()
	defer s.mu.Unlock()
	for pk, conns := range s.byKey {
		delete(conns, conn)
		if len(conns) == 0 {
			delete(s.byKey, pk)
		}
	}
}

// Query returns the live signals matching f, forgetting the stale ones.
func (s *Store) Query(f nostr.Filter) []nostr.Event {
	s.mu.Lock()
	defer s.mu.Unlock()
	var out []nostr.Event
	for pk, conns := range s.byKey {
		for conn, e := range conns {
			if s.Now().Sub(e.at) > TTL {
				delete(conns, conn)
			} else if f.Matches(e.ev) {
				out = append(out, e.ev)
			}
		}
		if len(conns) == 0 {
			delete(s.byKey, pk)
		}
	}
	return out
}
