// Package voice hands out LiveKit tokens and keeps voice channel presence
// (spec/protocol.md § 11): NIP-29 voice channels for community members, and
// end-to-end encrypted calls in DMs, whose key the instance never sees.
package voice

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/khatru"
	"github.com/golang-jwt/jwt/v5"

	"github.com/ErwanHeschung/ChatON/server/internal/community"
	"github.com/ErwanHeschung/ChatON/server/internal/handlers"
	"github.com/ErwanHeschung/ChatON/server/internal/models/dto"
	"github.com/ErwanHeschung/ChatON/server/internal/nip98"
)

const (
	WellKnown = "/.well-known/nip29/livekit" // GET: voice on? POST /<channel>: token
	CallPath  = "/api/voice/call"
	HookPath  = "/api/voice/webhook"
	ModPath   = "/api/voice/moderate"
	MoveTopic = "chaton.move" // the data message that moves a client (spec § 11)
	moveGrace = 15 * time.Second
	tokenTTL  = 10 * time.Minute
	maxCall   = 10 // members of a DM room (spec § 5)
)

var (
	groupRe  = regexp.MustCompile(`^[a-z0-9]{12}$`)
	callIDRe = regexp.MustCompile(`^[0-9a-f]{32}$`)
)

// Config is the LiveKit server next to the instance.
type Config struct {
	URL    string // wss:// address clients join
	Key    string // API key
	Secret string // API secret
}

type Directory interface {
	IsLocal(pubkey nostr.PubKey) bool
}

type Voice struct {
	cfg       Config
	api       string // the same server over http(s), for its Twirp API
	groups    *community.Groups
	dir       Directory
	publicURL string
	client    *http.Client
	now       func() time.Time

	mu    sync.Mutex
	rooms map[string]map[string]bool // LiveKit room → identities in it
	// What moderators set, by community then key (spec § 11, Modération vocale).
	// ponytail: in memory, a restart forgets it; store it if that bites.
	forced map[string]map[nostr.PubKey]forced
	// Keys being moved into a room, until when: the user limit lets them in.
	moving map[move]time.Time
}

type move struct {
	room string
	pk   nostr.PubKey
}

type forced struct{ muted, deafened bool }

func New(cfg Config, groups *community.Groups, dir Directory, publicURL string) *Voice {
	api := strings.Replace(strings.Replace(cfg.URL, "wss://", "https://", 1), "ws://", "http://", 1)
	return &Voice{
		cfg:       cfg,
		api:       strings.TrimSuffix(api, "/"),
		groups:    groups,
		dir:       dir,
		publicURL: strings.TrimSuffix(publicURL, "/"),
		client:    &http.Client{Timeout: 10 * time.Second},
		now:       time.Now,
		rooms:     map[string]map[string]bool{},
		forced:    map[string]map[nostr.PubKey]forced{},
		moving:    map[move]time.Time{},
	}
}

func (v *Voice) Register(mux *http.ServeMux) {
	mux.HandleFunc("GET "+WellKnown, func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	})
	mux.HandleFunc("POST "+WellKnown+"/{id}", v.channelToken)
	mux.HandleFunc("POST "+CallPath, v.callToken)
	mux.HandleFunc("POST "+HookPath, v.webhook)
	mux.HandleFunc("POST "+ModPath, v.moderate)
}

// Bind ejects members who are removed or leave from their community's voice channels.
func (v *Voice) Bind(rl *khatru.Relay) {
	before := rl.OnEventSaved
	rl.OnEventSaved = func(ctx context.Context, ev nostr.Event) {
		if before != nil {
			before(ctx, ev)
		}
		v.saved(ev)
	}
}

func (v *Voice) saved(ev nostr.Event) {
	var gone []nostr.PubKey
	switch ev.Kind {
	case nostr.KindSimpleGroupRemoveUser:
		for t := range ev.Tags.FindAll("p") {
			if pk, err := nostr.PubKeyFromHex(t[1]); err == nil {
				gone = append(gone, pk)
			}
		}
	case nostr.KindSimpleGroupLeaveRequest:
		gone = append(gone, ev.PubKey)
	case nostr.KindSimpleGroupPutUser: // a timeout: only view_channel is left
		t := ev.Tags.Find("timeout")
		if t == nil {
			return
		}
		if until, err := strconv.ParseInt(t[1], 10, 64); err != nil || until <= time.Now().Unix() {
			return
		}
		for t := range ev.Tags.FindAll("p") {
			if pk, err := nostr.PubKeyFromHex(t[1]); err == nil {
				gone = append(gone, pk)
			}
		}
	default:
		return
	}
	h := ev.Tags.Find("h")
	if h == nil || len(gone) == 0 {
		return
	}
	go v.eject(h[1], gone)
}

func (v *Voice) eject(community string, gone []nostr.PubKey) {
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	for _, room := range v.groups.VoiceChannels(community) {
		for _, identity := range v.identities(room) {
			if slices.ContainsFunc(gone, func(pk nostr.PubKey) bool { return strings.HasPrefix(identity, pk.Hex()) }) {
				body := map[string]string{"room": room, "identity": identity}
				if err := v.twirp(ctx, "RemoveParticipant", room, body, nil); err != nil {
					log.Printf("voice: eject %s from %s: %v", identity, room, err)
				}
			}
		}
	}
}

func (v *Voice) identities(room string) []string {
	v.mu.Lock()
	defer v.mu.Unlock()
	var ids []string
	for id := range v.rooms[room] {
		ids = append(ids, id)
	}
	return ids
}

// authed reads the body and checks its NIP-98 signature for this exact URL.
func (v *Voice) authed(w http.ResponseWriter, r *http.Request) (nostr.PubKey, []byte, bool) {
	body, err := io.ReadAll(io.LimitReader(r.Body, 4096))
	if err != nil {
		handlers.HandleError(w, dto.ErrInvalidInput("Unreadable body", err))
		return nostr.ZeroPK, nil, false
	}
	pubkey, err := nip98.Verify(r, v.publicURL+r.URL.Path, body, v.now())
	if err != nil {
		handlers.HandleError(w, dto.ErrUnauthorized("Invalid NIP-98 authorization", err))
		return nostr.ZeroPK, nil, false
	}
	return pubkey, body, true
}

func (v *Voice) channelToken(w http.ResponseWriter, r *http.Request) {
	pubkey, _, ok := v.authed(w, r)
	if !ok {
		return
	}
	room := r.PathValue("id")
	ch, voice := v.groups.Voice(room, pubkey)
	switch {
	case !voice:
		handlers.HandleError(w, dto.ErrNotFound("No such voice channel", nil))
	case !ch.Perms.Has("connect"):
		handlers.HandleError(w, dto.ErrForbidden("Not allowed to connect here", nil))
	case v.full(room, ch, pubkey):
		handlers.HandleError(w, dto.ErrConflict("This voice channel is full", nil))
	default:
		v.respond(w, pubkey, v.grant(room, ch, pubkey))
	}
}

// full: room has reached its user limit, and pk is neither in it nor exempt (move_members).
func (v *Voice) full(room string, ch community.VoiceChannel, pk nostr.PubKey) bool {
	if ch.UserLimit == 0 || ch.Perms.Has("move_members") {
		return false
	}
	v.mu.Lock()
	defer v.mu.Unlock()
	if until, ok := v.moving[move{room, pk}]; ok {
		delete(v.moving, move{room, pk})
		if v.now().Before(until) {
			return false
		}
	}
	present := keys(v.rooms[room])
	return !slices.Contains(present, pk) && len(present) >= ch.UserLimit
}

// grant: what pk may do in voice channel room, from its permissions there and what moderators set.
func (v *Voice) grant(room string, ch community.VoiceChannel, pk nostr.PubKey) grant {
	v.mu.Lock()
	f := v.forced[ch.Community][pk]
	v.mu.Unlock()
	var sources []string
	if ch.Perms.Has("speak") && !f.muted {
		sources = append(sources, "microphone")
	}
	if ch.Perms.Has("stream") {
		sources = append(sources, "screen_share")
	}
	publish, subscribe, no := len(sources) > 0, !f.deafened, false
	return grant{Room: room, RoomJoin: true, CanPublish: &publish, CanSubscribe: &subscribe, CanPublishData: &no, CanPublishSources: sources}
}

type moderateRequest struct {
	Channel string `json:"channel"`
	Target  string `json:"target"`
	Action  string `json:"action"`
	To      string `json:"to"`
}

// needs: the permission each moderation action asks for, in the channel.
var needs = map[string]string{
	"mute": "mute_members", "unmute": "mute_members", "deafen": "mute_members", "undeafen": "mute_members",
	"disconnect": "move_members", "move": "move_members",
}

// moderate: a moderator mutes, deafens, disconnects or moves someone in a voice channel.
func (v *Voice) moderate(w http.ResponseWriter, r *http.Request) {
	actor, body, ok := v.authed(w, r)
	if !ok {
		return
	}
	var req moderateRequest
	if err := json.Unmarshal(body, &req); err != nil || needs[req.Action] == "" {
		handlers.HandleError(w, dto.ErrInvalidInput("Expected {channel, target, action: mute|unmute|deafen|undeafen|disconnect|move, to?}", err))
		return
	}
	target, err := nostr.PubKeyFromHex(req.Target)
	if err != nil {
		handlers.HandleError(w, dto.ErrInvalidInput("Bad target key", err))
		return
	}
	ch, voice := v.groups.Voice(req.Channel, actor)
	switch {
	case !voice:
		handlers.HandleError(w, dto.ErrNotFound("No such voice channel", nil))
		return
	case !ch.Perms.Has(needs[req.Action]):
		handlers.HandleError(w, dto.ErrForbidden("Needs "+needs[req.Action], nil))
		return
	}
	ctx := r.Context()
	ids := v.identitiesOf(req.Channel, target)
	switch req.Action {
	case "mute", "unmute", "deafen", "undeafen":
		v.mu.Lock()
		if v.forced[ch.Community] == nil {
			v.forced[ch.Community] = map[nostr.PubKey]forced{}
		}
		f := v.forced[ch.Community][target]
		switch req.Action {
		case "mute", "unmute":
			f.muted = req.Action == "mute"
		default:
			f.deafened = req.Action == "deafen"
		}
		v.forced[ch.Community][target] = f
		if f == (forced{}) {
			delete(v.forced[ch.Community], target)
		}
		v.mu.Unlock()
		v.regrant(ctx, req.Channel, target, ids)
		v.publish(ctx, req.Channel)
	case "disconnect":
		if len(ids) == 0 {
			handlers.HandleError(w, dto.ErrNotFound("Not in this voice channel", nil))
			return
		}
		for _, id := range ids {
			if err := v.twirp(ctx, "RemoveParticipant", req.Channel, map[string]string{"room": req.Channel, "identity": id}, nil); err != nil {
				handlers.HandleError(w, err)
				return
			}
		}
	case "move":
		to, voice := v.groups.Voice(req.To, actor)
		theirs, _ := v.groups.Voice(req.To, target)
		switch {
		case !voice || to.Community != ch.Community || req.To == req.Channel:
			handlers.HandleError(w, dto.ErrNotFound("No such voice channel to move to", nil))
			return
		case !to.Perms.Has("move_members") || !theirs.Perms.Has("connect"):
			handlers.HandleError(w, dto.ErrForbidden("Needs move_members there, and them connect", nil))
			return
		case len(ids) == 0:
			handlers.HandleError(w, dto.ErrNotFound("Not in this voice channel", nil))
			return
		}
		// Self-hosted LiveKit has no MoveParticipant: the client moves itself,
		// told by a data message only the server can send (canPublishData is
		// off for everyone), and the limit lets it in for a while.
		v.mu.Lock()
		v.moving[move{req.To, target}] = v.now().Add(moveGrace)
		v.mu.Unlock()
		data, _ := json.Marshal(map[string]string{"community": ch.Community, "channel": req.To})
		body := map[string]any{"room": req.Channel, "data": data, "kind": "RELIABLE", "destination_identities": ids, "topic": MoveTopic}
		if err := v.twirp(ctx, "SendData", req.Channel, body, nil); err != nil {
			handlers.HandleError(w, err)
			return
		}
		// A client that doesn't follow leaves anyway (a new device would have a new identity).
		time.AfterFunc(moveGrace, func() { v.remove(req.Channel, ids) })
	}
	w.WriteHeader(http.StatusNoContent)
}

// regrant gives target's devices (ids) in room what they may do there now.
func (v *Voice) regrant(ctx context.Context, room string, target nostr.PubKey, ids []string) {
	ch, ok := v.groups.Voice(room, target)
	if !ok {
		return
	}
	g := v.grant(room, ch, target)
	sources := []string{} // the API's names, not the token's
	for _, s := range g.CanPublishSources {
		sources = append(sources, strings.ToUpper(s))
	}
	permission := map[string]any{
		"canPublish": *g.CanPublish, "canSubscribe": *g.CanSubscribe, "canPublishData": false, "canPublishSources": sources,
	}
	for _, id := range ids {
		body := map[string]any{"room": room, "identity": id, "permission": permission}
		if err := v.twirp(ctx, "UpdateParticipant", room, body, nil); err != nil {
			log.Printf("voice: permissions of %s in %s: %v", id, room, err)
		}
	}
}

// remove takes ids out of room, those still in it.
func (v *Voice) remove(room string, ids []string) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	present := v.identities(room)
	for _, id := range ids {
		if slices.Contains(present, id) {
			if err := v.twirp(ctx, "RemoveParticipant", room, map[string]string{"room": room, "identity": id}, nil); err != nil {
				log.Printf("voice: remove %s from %s: %v", id, room, err)
			}
		}
	}
}

// identitiesOf: pk's devices in room.
func (v *Voice) identitiesOf(room string, pk nostr.PubKey) []string {
	return slices.DeleteFunc(v.identities(room), func(id string) bool { return !strings.HasPrefix(id, pk.Hex()+":") })
}

// publish: who is in voice channel room, and who of them is muted or deafened (39004).
func (v *Voice) publish(ctx context.Context, room string) {
	ch, ok := v.groups.Voice(room, nostr.ZeroPK)
	if !ok {
		return
	}
	v.mu.Lock()
	pks := keys(v.rooms[room])
	var muted, deafened []nostr.PubKey
	for _, pk := range pks {
		f := v.forced[ch.Community][pk]
		if f.muted {
			muted = append(muted, pk)
		}
		if f.deafened {
			deafened = append(deafened, pk)
		}
	}
	v.mu.Unlock()
	v.groups.SetVoiceParticipants(ctx, room, pks, muted, deafened)
}

type callRequest struct {
	ID      string   `json:"id"`
	Members []string `json:"members"`
}

// callToken: a DM call's room is derived from its id and members, nothing is
// stored. The E2EE key never comes here (spec § 11).
func (v *Voice) callToken(w http.ResponseWriter, r *http.Request) {
	pubkey, body, ok := v.authed(w, r)
	if !ok {
		return
	}
	var req callRequest
	if err := json.Unmarshal(body, &req); err != nil || !callIDRe.MatchString(req.ID) {
		handlers.HandleError(w, dto.ErrInvalidInput("Expected {id: 32 hex, members: [hex keys]}", err))
		return
	}
	var members []nostr.PubKey
	for _, m := range req.Members {
		pk, err := nostr.PubKeyFromHex(m)
		if err != nil {
			handlers.HandleError(w, dto.ErrInvalidInput("Bad member key", err))
			return
		}
		if !slices.Contains(members, pk) {
			members = append(members, pk)
		}
	}
	switch {
	case len(members) < 2 || len(members) > maxCall:
		handlers.HandleError(w, dto.ErrInvalidInput(fmt.Sprintf("A call has 2 to %d members", maxCall), nil))
	case !slices.Contains(members, pubkey):
		handlers.HandleError(w, dto.ErrForbidden("Not a member of this call", nil))
	case !slices.ContainsFunc(members, v.dir.IsLocal):
		handlers.HandleError(w, dto.ErrForbidden("No member of this call has a handle here", nil))
	default:
		yes, no := true, false
		v.respond(w, pubkey, grant{
			Room: CallRoom(req.ID, members), RoomJoin: true, CanPublish: &yes, CanSubscribe: &yes, CanPublishData: &no,
			CanPublishSources: []string{"microphone", "screen_share"},
		})
	}
}

// CallRoom: "call-" + hex(sha256("chaton-call-v0|" + id + "|" + sorted keys joined by ",")).
func CallRoom(id string, members []nostr.PubKey) string {
	keys := make([]string, len(members))
	for i, pk := range members {
		keys[i] = pk.Hex()
	}
	slices.Sort(keys)
	sum := sha256.Sum256([]byte("chaton-call-v0|" + id + "|" + strings.Join(keys, ",")))
	return "call-" + hex.EncodeToString(sum[:])
}

func (v *Voice) respond(w http.ResponseWriter, pubkey nostr.PubKey, g grant) {
	suffix := make([]byte, 4)
	_, _ = rand.Read(suffix)
	token, err := v.token(pubkey.Hex()+":"+hex.EncodeToString(suffix), g)
	if err != nil {
		handlers.HandleError(w, err)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusCreated)
	_ = json.NewEncoder(w).Encode(map[string]string{"server_url": v.cfg.URL, "participant_token": token})
}

// grant is LiveKit's "video" claim.
type grant struct {
	Room              string   `json:"room,omitempty"`
	RoomJoin          bool     `json:"roomJoin,omitempty"`
	RoomAdmin         bool     `json:"roomAdmin,omitempty"`
	RoomList          bool     `json:"roomList,omitempty"`
	CanPublish        *bool    `json:"canPublish,omitempty"`
	CanSubscribe      *bool    `json:"canSubscribe,omitempty"`
	CanPublishData    *bool    `json:"canPublishData,omitempty"`
	CanPublishSources []string `json:"canPublishSources,omitempty"`
}

// token is a LiveKit access token: an HS256 JWT signed with the API secret.
func (v *Voice) token(identity string, g grant) (string, error) {
	now := v.now()
	claims := jwt.MapClaims{
		"iss":   v.cfg.Key,
		"nbf":   now.Unix(),
		"exp":   now.Add(tokenTTL).Unix(),
		"video": g,
	}
	if identity != "" {
		claims["sub"] = identity
	}
	signed, err := jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString([]byte(v.cfg.Secret))
	if err != nil {
		return "", fmt.Errorf("livekit token: %w", err)
	}
	return signed, nil
}

type hook struct {
	Event       string                    `json:"event"`
	Room        struct{ Name string }     `json:"room"`
	Participant struct{ Identity string } `json:"participant"`
}

// webhook: LiveKit tells who joins and leaves; voice channels get a new 39004.
// Signed with the API secret, with the body's sha256 in the token.
func (v *Voice) webhook(w http.ResponseWriter, r *http.Request) {
	body, err := io.ReadAll(io.LimitReader(r.Body, 64<<10))
	if err != nil {
		handlers.HandleError(w, dto.ErrInvalidInput("Unreadable body", err))
		return
	}
	if err := v.checkHook(r.Header.Get("Authorization"), body); err != nil {
		handlers.HandleError(w, dto.ErrUnauthorized("Invalid webhook signature", err))
		return
	}
	var ev hook
	if err := json.Unmarshal(body, &ev); err != nil {
		handlers.HandleError(w, dto.ErrInvalidInput("Bad webhook body", err))
		return
	}
	room, identity := ev.Room.Name, ev.Participant.Identity
	v.mu.Lock()
	switch ev.Event {
	case "participant_joined":
		if v.rooms[room] == nil {
			v.rooms[room] = map[string]bool{}
		}
		v.rooms[room][identity] = true
	case "participant_left", "participant_connection_aborted":
		delete(v.rooms[room], identity)
	case "room_finished":
		delete(v.rooms, room)
	default:
		v.mu.Unlock()
		w.WriteHeader(http.StatusNoContent)
		return
	}
	v.mu.Unlock()
	if groupRe.MatchString(room) { // DM calls ("call-…") publish nothing
		v.publish(r.Context(), room)
	}
	w.WriteHeader(http.StatusNoContent)
}

func (v *Voice) checkHook(auth string, body []byte) error {
	claims := jwt.MapClaims{}
	_, err := jwt.ParseWithClaims(strings.TrimPrefix(auth, "Bearer "), claims,
		func(*jwt.Token) (any, error) { return []byte(v.cfg.Secret), nil },
		jwt.WithValidMethods([]string{"HS256"}), jwt.WithIssuer(v.cfg.Key), jwt.WithTimeFunc(v.now))
	if err != nil {
		return err
	}
	sum := sha256.Sum256(body)
	want, _ := claims["sha256"].(string)
	if subtle.ConstantTimeCompare([]byte(want), []byte(base64.StdEncoding.EncodeToString(sum[:]))) != 1 {
		return errors.New("body hash mismatch")
	}
	return nil
}

// keys: the distinct public keys behind identities ("<hex>:<suffix>", spec § 11).
func keys(identities map[string]bool) []nostr.PubKey {
	pks := []nostr.PubKey{}
	for id := range identities {
		hexKey, _, _ := strings.Cut(id, ":")
		if pk, err := nostr.PubKeyFromHex(hexKey); err == nil && !slices.Contains(pks, pk) {
			pks = append(pks, pk)
		}
	}
	slices.SortFunc(pks, func(a, b nostr.PubKey) int { return bytes.Compare(a[:], b[:]) })
	return pks
}

// Reconcile reads who is in which voice channel from LiveKit (at startup:
// webhooks missed while down), and republishes every voice channel's 39004.
func (v *Voice) Reconcile(ctx context.Context) {
	var rooms struct{ Rooms []struct{ Name string } }
	if err := v.twirp(ctx, "ListRooms", "", map[string]any{}, &rooms); err != nil {
		log.Printf("voice: LiveKit unreachable, presence starts empty: %v", err)
	}
	for _, room := range rooms.Rooms {
		if !groupRe.MatchString(room.Name) {
			continue
		}
		var list struct{ Participants []struct{ Identity string } }
		if err := v.twirp(ctx, "ListParticipants", room.Name, map[string]string{"room": room.Name}, &list); err != nil {
			log.Printf("voice: participants of %s: %v", room.Name, err)
			continue
		}
		v.mu.Lock()
		v.rooms[room.Name] = map[string]bool{}
		for _, p := range list.Participants {
			v.rooms[room.Name][p.Identity] = true
		}
		v.mu.Unlock()
	}
	for _, id := range v.groups.VoiceChannels("") {
		v.publish(ctx, id)
	}
}

// twirp calls LiveKit's RoomService (JSON over HTTP) as an admin of room ("" = list rooms).
func (v *Voice) twirp(ctx context.Context, method, room string, in, out any) error {
	token, err := v.token("", grant{Room: room, RoomAdmin: room != "", RoomList: room == ""})
	if err != nil {
		return err
	}
	raw, err := json.Marshal(in)
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, v.api+"/twirp/livekit.RoomService/"+method, bytes.NewReader(raw))
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("Content-Type", "application/json")
	resp, err := v.client.Do(req)
	if err != nil {
		return fmt.Errorf("livekit %s: %w", method, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("livekit %s: status %d", method, resp.StatusCode)
	}
	if out == nil {
		return nil
	}
	if err := json.NewDecoder(resp.Body).Decode(out); err != nil {
		return fmt.Errorf("livekit %s: %w", method, err)
	}
	return nil
}
