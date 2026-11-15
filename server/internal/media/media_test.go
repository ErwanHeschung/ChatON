package media_test

import (
	"bytes"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/eventstore/boltdb"

	"github.com/ErwanHeschung/ChatON/server/internal/community"
	"github.com/ErwanHeschung/ChatON/server/internal/media"
	"github.com/ErwanHeschung/ChatON/server/internal/relay"
)

type fakeDir map[nostr.PubKey]bool

func (d fakeDir) IsLocal(pk nostr.PubKey) bool { return d[pk] }

// serve runs relay + Blossom like cmd/server, with local users.
func serve(t *testing.T, locals ...nostr.SecretKey) string {
	t.Helper()
	tmp := t.TempDir()
	open := func(name string) *boltdb.BoltBackend {
		db := &boltdb.BoltBackend{Path: filepath.Join(tmp, name)}
		if err := db.Init(); err != nil {
			t.Fatal(err)
		}
		t.Cleanup(db.Close)
		return db
	}
	db, index := open("r.bolt"), open("blobs.bolt")
	dir := fakeDir{}
	for _, sk := range locals {
		dir[sk.Public()] = true
	}
	groups, err := community.New(nostr.Generate(), "ws://test", db, dir)
	if err != nil {
		t.Fatal(err)
	}
	rl := relay.New(relay.Config{Name: "test"}, db, dir, groups)
	srv := httptest.NewUnstartedServer(media.Guard(rl))
	srv.Start()
	t.Cleanup(srv.Close)
	if _, err := media.New(rl, srv.URL, dir, filepath.Join(tmp, "blobs"), index); err != nil {
		t.Fatal(err)
	}
	return srv.URL
}

// auth is a Blossom authorization header (kind 24242, BUD-01).
func auth(t *testing.T, sk nostr.SecretKey, verb, hash string) string {
	t.Helper()
	ev := nostr.Event{
		Kind:      24242,
		CreatedAt: nostr.Now(),
		Tags:      nostr.Tags{{"t", verb}, {"x", hash}, {"expiration", strconv.FormatInt(int64(nostr.Now())+60, 10)}},
	}
	if err := ev.Sign(sk); err != nil {
		t.Fatal(err)
	}
	j, _ := json.Marshal(ev)
	return "Nostr " + base64.RawURLEncoding.EncodeToString(j)
}

func do(t *testing.T, method, url, authorization string, body []byte) (int, []byte) {
	t.Helper()
	req, _ := http.NewRequest(method, url, bytes.NewReader(body))
	if authorization != "" {
		req.Header.Set("Authorization", authorization)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	got, _ := io.ReadAll(resp.Body)
	return resp.StatusCode, got
}

func TestBlossom(t *testing.T) {
	alice, bob, stranger := nostr.Generate(), nostr.Generate(), nostr.Generate()
	url := serve(t, alice, bob)
	file := []byte("ciphertext, as the client would send it")
	sum := sha256.Sum256(file)
	hash := hex.EncodeToString(sum[:])

	if code, body := do(t, "PUT", url+"/upload", auth(t, stranger, "upload", hash), file); code != 403 {
		t.Errorf("upload without a handle here: %d %s", code, body)
	}
	if code, body := do(t, "PUT", url+"/upload", "", file); code != 401 {
		t.Errorf("upload without auth: %d %s", code, body)
	}

	code, body := do(t, "PUT", url+"/upload", auth(t, alice, "upload", hash), file)
	if code != 200 {
		t.Fatalf("upload: %d %s", code, body)
	}
	var desc struct{ URL, SHA256 string }
	_ = json.Unmarshal(body, &desc)
	if desc.SHA256 != hash || !strings.HasPrefix(desc.URL, url+"/"+hash) {
		t.Errorf("descriptor: %s", body)
	}
	if code, got := do(t, "GET", url+"/"+hash, "", nil); code != 200 || !bytes.Equal(got, file) {
		t.Errorf("get by hash, no auth: %d %q", code, got)
	}

	// Lists are private; only the uploader deletes.
	if code, _ := do(t, "GET", url+"/list/"+alice.Public().Hex(), auth(t, bob, "list", hash), nil); code != 403 {
		t.Errorf("bob lists alice's files: %d", code)
	}
	if code, _ := do(t, "GET", url+"/list/"+alice.Public().Hex(), auth(t, alice, "list", hash), nil); code != 200 {
		t.Errorf("alice lists her files: %d", code)
	}
	do(t, "DELETE", url+"/"+hash, auth(t, bob, "delete", hash), nil)
	if code, _ := do(t, "GET", url+"/"+hash, "", nil); code != 200 {
		t.Errorf("deleted by someone else: %d", code)
	}
	do(t, "DELETE", url+"/"+hash, auth(t, alice, "delete", hash), nil)
	if code, _ := do(t, "GET", url+"/"+hash, "", nil); code != 404 {
		t.Errorf("deleted by alice, still served: %d", code)
	}

	// The relay still works behind the same handler.
	if code, _ := do(t, "GET", url+"/health-does-not-exist", "", nil); code != 404 {
		t.Errorf("unknown path: %d", code)
	}
}

func TestGuard(t *testing.T) {
	reached := false
	guard := media.Guard(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		reached = true
		if _, err := io.ReadAll(r.Body); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
		}
	}))
	run := func(method, path, length, body string) int {
		reached = false
		r := httptest.NewRequest(method, path, strings.NewReader(body))
		r.Header.Set("Content-Length", length)
		w := httptest.NewRecorder()
		guard.ServeHTTP(w, r)
		return w.Code
	}

	if code := run("PUT", "/upload", strconv.Itoa(media.MaxBlobSize+1), "x"); code != 413 || reached {
		t.Errorf("oversize Content-Length: %d, reached %v", code, reached)
	}
	if code := run("PUT", "/upload", "", "x"); code != 411 || reached {
		t.Errorf("no Content-Length: %d, reached %v", code, reached)
	}
	if code := run("PUT", "/upload", "3", "longer than announced"); code != 400 {
		t.Errorf("body longer than announced: %d", code)
	}
	for _, path := range []string{"/mirror", "/media", "/report"} {
		if code := run("PUT", path, "1", "x"); code != 404 || reached {
			t.Errorf("%s: %d, reached %v", path, code, reached)
		}
	}
	if code := run("PUT", "/upload", "1", "x"); code != 200 || !reached {
		t.Errorf("normal upload blocked: %d", code)
	}
}
