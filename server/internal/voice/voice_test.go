package voice_test

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/eventstore/boltdb"
	"github.com/golang-jwt/jwt/v5"

	"github.com/ErwanHeschung/ChatON/server/internal/community"
	"github.com/ErwanHeschung/ChatON/server/internal/relay"
	"github.com/ErwanHeschung/ChatON/server/internal/voice"
)

const (
	apiKey, apiSecret         = "devkey", "a-dev-secret-of-at-least-32-chars!"
	communityID, textID, room = "k3j9x2m8q0aa", "g7w2p5n1c4bb", "v8t3r6m2d5cc"
)

type fakeDir map[nostr.PubKey]bool

func (d fakeDir) IsLocal(pk nostr.PubKey) bool { return d[pk] }

// livekit fakes the RoomService API: it records calls and answers ListRooms/ListParticipants.
type livekit struct {
	mu      sync.Mutex
	calls   []string // "<method> <body>"
	rooms   []string
	present map[string][]string // room → identities
}

func (lk *livekit) serve(t *testing.T) string {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		claims := jwt.MapClaims{}
		if _, err := jwt.ParseWithClaims(strings.TrimPrefix(r.Header.Get("Authorization"), "Bearer "), claims,
			func(*jwt.Token) (any, error) { return []byte(apiSecret), nil }); err != nil {
			t.Errorf("twirp token: %v", err)
		}
		body, _ := io.ReadAll(r.Body)
		method := strings.TrimPrefix(r.URL.Path, "/twirp/livekit.RoomService/")
		lk.mu.Lock()
		defer lk.mu.Unlock()
		lk.calls = append(lk.calls, method+" "+string(body))
		switch method {
		case "ListRooms":
			var rooms []map[string]string
			for _, name := range lk.rooms {
				rooms = append(rooms, map[string]string{"name": name})
			}
			_ = json.NewEncoder(w).Encode(map[string]any{"rooms": rooms})
		case "ListParticipants":
			var in struct{ Room string }
			_ = json.Unmarshal(body, &in)
			var list []map[string]string
			for _, id := range lk.present[in.Room] {
				list = append(list, map[string]string{"identity": id})
			}
			_ = json.NewEncoder(w).Encode(map[string]any{"participants": list})
		default:
			_, _ = w.Write([]byte("{}"))
		}
	}))
	t.Cleanup(srv.Close)
	return "ws" + strings.TrimPrefix(srv.URL, "http")
}

func (lk *livekit) called(prefix string) bool {
	lk.mu.Lock()
	defer lk.mu.Unlock()
	return slices.ContainsFunc(lk.calls, func(c string) bool { return strings.HasPrefix(c, prefix) })
}

type user struct {
	sk nostr.SecretKey
	pk nostr.PubKey
}

func newUser() user {
	sk := nostr.Generate()
	return user{sk, sk.Public()}
}

type instance struct {
	http, ws string
	relayKey nostr.PubKey
	voice    *voice.Voice
	lk       *livekit
}

// serve runs a relay with voice, next to a fake LiveKit; locals have a handle.
func serve(t *testing.T, locals ...user) instance {
	t.Helper()
	db := &boltdb.BoltBackend{Path: filepath.Join(t.TempDir(), "r.bolt")}
	if err := db.Init(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(db.Close)
	dir := fakeDir{}
	for _, u := range locals {
		dir[u.pk] = true
	}
	key := nostr.Generate()
	groups, err := community.New(key, "ws://test", db, dir)
	if err != nil {
		t.Fatal(err)
	}
	rl := relay.New(relay.Config{Name: "test"}, db, dir, groups)
	srv := httptest.NewServer(rl)
	t.Cleanup(srv.Close)
	lk := &livekit{present: map[string][]string{}}
	v := voice.New(voice.Config{URL: lk.serve(t), Key: apiKey, Secret: apiSecret}, groups, dir, srv.URL)
	v.Bind(rl)
	v.Register(rl.Router())
	return instance{srv.URL, "ws" + strings.TrimPrefix(srv.URL, "http"), key.Public(), v, lk}
}

func connect(t *testing.T, url string, u *user) *nostr.Relay {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	r, err := nostr.RelayConnect(ctx, url, nostr.RelayOptions{})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { r.Close() })
	for u != nil { // the AUTH challenge comes right after connecting
		if err = r.Auth(ctx, func(_ context.Context, ev *nostr.Event) error { return ev.Sign(u.sk) }); err == nil {
			break
		}
		if ctx.Err() != nil {
			t.Fatalf("auth: %v", err)
		}
		time.Sleep(20 * time.Millisecond)
	}
	return r
}

func publish(t *testing.T, r *nostr.Relay, u user, kind nostr.Kind, tags ...nostr.Tag) {
	t.Helper()
	ev := nostr.Event{Kind: kind, Tags: tags, CreatedAt: nostr.Now()}
	_ = ev.Sign(u.sk)
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	if err := r.Publish(ctx, ev); err != nil {
		t.Fatalf("kind %d: %v", kind, err)
	}
}

// community: alice (admin) and bob (member), a text channel and a voice channel.
func setupCommunity(t *testing.T, in instance, alice, bob user) {
	t.Helper()
	a, b := connect(t, in.ws, &alice), connect(t, in.ws, &bob)
	h := func(id string) nostr.Tag { return nostr.Tag{"h", id} }
	publish(t, a, alice, nostr.KindSimpleGroupCreateGroup, h(communityID))
	for _, ch := range []struct {
		id    string
		extra []nostr.Tag
	}{{textID, nil}, {room, []nostr.Tag{{"livekit"}}}} {
		publish(t, a, alice, nostr.KindSimpleGroupCreateGroup, append([]nostr.Tag{h(ch.id), {"name", ch.id}, {"parent", communityID}}, ch.extra...)...)
	}
	publish(t, a, alice, nostr.KindSimpleGroupCreateInvite, h(communityID), nostr.Tag{"code", "AB12CD34"})
	publish(t, b, bob, nostr.KindSimpleGroupJoinRequest, h(communityID), nostr.Tag{"code", "AB12CD34"})
}

// post sends a NIP-98 signed POST (unsigned when u is nil); returns status and JSON body.
func post(t *testing.T, in instance, path string, u *user, body any) (int, map[string]string) {
	t.Helper()
	raw, _ := json.Marshal(body)
	req, _ := http.NewRequest(http.MethodPost, in.http+path, bytes.NewReader(raw))
	if u != nil {
		sum := sha256.Sum256(raw)
		ev := nostr.Event{Kind: nostr.KindHTTPAuth, CreatedAt: nostr.Now(), Tags: nostr.Tags{
			{"u", in.http + path}, {"method", http.MethodPost}, {"payload", hex.EncodeToString(sum[:])},
		}}
		_ = ev.Sign(u.sk)
		j, _ := json.Marshal(ev)
		req.Header.Set("Authorization", "Nostr "+base64.StdEncoding.EncodeToString(j))
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	out := map[string]string{}
	_ = json.NewDecoder(resp.Body).Decode(&out)
	return resp.StatusCode, out
}

// claims checks a participant token with the API secret.
func claims(t *testing.T, token string) jwt.MapClaims {
	t.Helper()
	c := jwt.MapClaims{}
	if _, err := jwt.ParseWithClaims(token, c, func(*jwt.Token) (any, error) { return []byte(apiSecret), nil },
		jwt.WithValidMethods([]string{"HS256"}), jwt.WithIssuer(apiKey)); err != nil {
		t.Fatalf("token: %v", err)
	}
	return c
}

// hook sends a LiveKit webhook, signed like LiveKit does (or with secret).
func hook(t *testing.T, in instance, secret, event, roomName, identity string) int {
	t.Helper()
	raw, _ := json.Marshal(map[string]any{"event": event, "room": map[string]string{"name": roomName},
		"participant": map[string]string{"identity": identity}})
	sum := sha256.Sum256(raw)
	token, _ := jwt.NewWithClaims(jwt.SigningMethodHS256, jwt.MapClaims{
		"iss": apiKey, "sha256": base64.StdEncoding.EncodeToString(sum[:]), "exp": time.Now().Add(time.Minute).Unix(),
	}).SignedString([]byte(secret))
	req, _ := http.NewRequest(http.MethodPost, in.http+voice.HookPath, bytes.NewReader(raw))
	req.Header.Set("Authorization", token)
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	_ = resp.Body.Close()
	return resp.StatusCode
}

// participants reads the relay-signed 39004 of the voice channel (public, like 39000).
func participants(t *testing.T, in instance) []string {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	var out []string
	for ev := range connect(t, in.ws, nil).QueryEvents(nostr.Filter{
		Kinds: []nostr.Kind{nostr.KindSimpleGroupLiveKitParticipants}, Tags: nostr.TagMap{"d": {room}},
	}) {
		if ev.PubKey != in.relayKey {
			t.Errorf("39004 signed by %s, not the relay", ev.PubKey)
		}
		for tag := range ev.Tags.FindAll("participant") {
			out = append(out, tag[1])
		}
		cancel()
	}
	_ = ctx
	slices.Sort(out)
	return out
}

func TestChannelToken(t *testing.T) {
	alice, bob, carol := newUser(), newUser(), newUser()
	in := serve(t, alice, bob, carol)
	setupCommunity(t, in, alice, bob)

	resp, err := http.Get(in.http + voice.WellKnown)
	if err != nil || resp.StatusCode != http.StatusNoContent {
		t.Fatalf("voice on: %v %v", resp, err)
	}
	status, out := post(t, in, voice.WellKnown+"/"+room, &bob, map[string]any{})
	if status != http.StatusCreated || out["server_url"] == "" {
		t.Fatalf("member: %d %v", status, out)
	}
	c := claims(t, out["participant_token"])
	video, _ := c["video"].(map[string]any)
	sub, _ := c["sub"].(string)
	if !strings.HasPrefix(sub, bob.pk.Hex()+":") || len(sub) != 64+1+8 || video["room"] != room || video["roomJoin"] != true ||
		video["canPublishData"] != false || !slices.Equal(video["canPublishSources"].([]any), []any{"microphone", "screen_share"}) {
		t.Errorf("claims: sub %q video %v", sub, video)
	}
	if exp, _ := c.GetExpirationTime(); exp == nil || time.Until(exp.Time) > 11*time.Minute {
		t.Errorf("expiry: %v", exp)
	}

	for name, tc := range map[string]struct {
		path string
		u    *user
		want int
	}{
		"unsigned":     {"/" + room, nil, http.StatusUnauthorized},
		"non-member":   {"/" + room, &carol, http.StatusForbidden},
		"text channel": {"/" + textID, &bob, http.StatusNotFound},
		"community":    {"/" + communityID, &alice, http.StatusNotFound},
		"unknown":      {"/zzzzzzzzzzzz", &alice, http.StatusNotFound},
	} {
		if status, _ := post(t, in, voice.WellKnown+tc.path, tc.u, map[string]any{}); status != tc.want {
			t.Errorf("%s: %d, want %d", name, status, tc.want)
		}
	}
}

func TestCallToken(t *testing.T) {
	alice, bob, carol, dave := newUser(), newUser(), newUser(), newUser()
	in := serve(t, alice) // bob, carol, dave: other instances
	id := strings.Repeat("0f", 16)
	members := []string{alice.pk.Hex(), bob.pk.Hex()}

	status, out := post(t, in, voice.CallPath, &bob, map[string]any{"id": id, "members": members})
	if status != http.StatusCreated {
		t.Fatalf("callee from another instance: %d", status)
	}
	want := voice.CallRoom(id, []nostr.PubKey{bob.pk, alice.pk})
	if video, _ := claims(t, out["participant_token"])["video"].(map[string]any); video["room"] != want {
		t.Errorf("room %v, want %s", video["room"], want)
	}
	if voice.CallRoom(id, []nostr.PubKey{alice.pk, carol.pk}) == want || voice.CallRoom(strings.Repeat("1f", 16), []nostr.PubKey{alice.pk, bob.pk}) == want {
		t.Error("the room must change with the members and the id")
	}

	eleven := []string{alice.pk.Hex()}
	for range 10 {
		eleven = append(eleven, newUser().pk.Hex())
	}
	for name, tc := range map[string]struct {
		u    *user
		body map[string]any
		want int
	}{
		"unsigned":       {nil, map[string]any{"id": id, "members": members}, http.StatusUnauthorized},
		"not a member":   {&carol, map[string]any{"id": id, "members": members}, http.StatusForbidden},
		"no local":       {&carol, map[string]any{"id": id, "members": []string{carol.pk.Hex(), dave.pk.Hex()}}, http.StatusForbidden},
		"alone":          {&alice, map[string]any{"id": id, "members": []string{alice.pk.Hex(), alice.pk.Hex()}}, http.StatusBadRequest},
		"eleven members": {&alice, map[string]any{"id": id, "members": eleven}, http.StatusBadRequest},
		"bad id":         {&alice, map[string]any{"id": "nope", "members": members}, http.StatusBadRequest},
	} {
		if status, _ := post(t, in, voice.CallPath, tc.u, tc.body); status != tc.want {
			t.Errorf("%s: %d, want %d", name, status, tc.want)
		}
	}
}

func TestPresence(t *testing.T) {
	alice, bob := newUser(), newUser()
	in := serve(t, alice, bob)
	setupCommunity(t, in, alice, bob)
	ida, ida2, idb := alice.pk.Hex()+":aaaaaaaa", alice.pk.Hex()+":bbbbbbbb", bob.pk.Hex()+":cccccccc"

	if status := hook(t, in, "wrong-secret-wrong-secret-wrong!!", "participant_joined", room, ida); status != http.StatusUnauthorized {
		t.Fatalf("bad signature: %d", status)
	}
	for _, id := range []string{ida, ida2, idb} { // alice on two devices
		if status := hook(t, in, apiSecret, "participant_joined", room, id); status != http.StatusNoContent {
			t.Fatalf("joined: %d", status)
		}
	}
	both := []string{alice.pk.Hex(), bob.pk.Hex()}
	slices.Sort(both)
	if got := participants(t, in); !slices.Equal(got, both) {
		t.Fatalf("participants %v, want %v", got, both)
	}
	hook(t, in, apiSecret, "participant_left", room, ida)
	if got := participants(t, in); !slices.Equal(got, both) {
		t.Errorf("alice still has a device in: %v", got)
	}
	hook(t, in, apiSecret, "participant_left", room, ida2)
	if got := participants(t, in); !slices.Equal(got, []string{bob.pk.Hex()}) {
		t.Errorf("after alice left: %v", got)
	}
	hook(t, in, apiSecret, "room_finished", room, "")
	if got := participants(t, in); len(got) != 0 {
		t.Errorf("after the room finished: %v", got)
	}

	// Bob is removed while in the channel: LiveKit is told to eject him.
	hook(t, in, apiSecret, "participant_joined", room, idb)
	a := connect(t, in.ws, &alice)
	publish(t, a, alice, nostr.KindSimpleGroupRemoveUser, nostr.Tag{"h", communityID}, nostr.Tag{"p", bob.pk.Hex()})
	deadline := time.Now().Add(3 * time.Second)
	for !in.lk.called(`RemoveParticipant {"identity":"` + idb + `","room":"` + room + `"}`) {
		if time.Now().After(deadline) {
			t.Fatalf("no RemoveParticipant: %v", in.lk.calls)
		}
		time.Sleep(20 * time.Millisecond)
	}
}

func TestReconcile(t *testing.T) {
	alice, bob := newUser(), newUser()
	in := serve(t, alice, bob)
	setupCommunity(t, in, alice, bob)
	in.lk.rooms = []string{room, "call-abc"}
	in.lk.present[room] = []string{alice.pk.Hex() + ":aaaaaaaa"}

	in.voice.Reconcile(context.Background())
	if got := participants(t, in); !slices.Equal(got, []string{alice.pk.Hex()}) {
		t.Errorf("after reconcile: %v", got)
	}
	if in.lk.called(`ListParticipants {"room":"call-abc"}`) {
		t.Error("DM call rooms have no presence")
	}
}
