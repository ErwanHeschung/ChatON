// Package directory maps pseudo@instance to a public key (NIP-05) and lets a
// key claim a free handle with a NIP-98 signed request. See spec/federation.md.
package directory

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"regexp"
	"strings"
	"time"

	"fiatjaf.com/nostr"
	"github.com/go-playground/validator/v10"
	"go.etcd.io/bbolt"

	"github.com/ErwanHeschung/ChatON/server/internal/handlers"
	"github.com/ErwanHeschung/ChatON/server/internal/models/dto"
	"github.com/ErwanHeschung/ChatON/server/internal/nip98"
)

const ClaimPath = "/api/handle"

var nameRe = regexp.MustCompile(`^[a-z0-9._-]{3,32}$`) // also rules out "_" (NIP-05 instance key)

// Two buckets in the relay's bbolt file, written together: name → pubkey, pubkey → name.
var (
	byName   = []byte("handles")
	byPubKey = []byte("handlesByPubkey")
)

var errTaken = errors.New("name taken, or this key already has a handle")

type Directory struct {
	db        *bbolt.DB
	publicURL string // https://chaton.example
	relayURL  string // wss://chaton.example
	validate  *validator.Validate
	now       func() time.Time
}

func New(db *bbolt.DB, publicURL string) (*Directory, error) {
	err := db.Update(func(tx *bbolt.Tx) error {
		for _, b := range [][]byte{byName, byPubKey} {
			if _, err := tx.CreateBucketIfNotExists(b); err != nil {
				return err
			}
		}
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("directory buckets: %w", err)
	}
	publicURL = strings.TrimSuffix(publicURL, "/")
	relayURL := "ws" + strings.TrimPrefix(publicURL, "http") // http→ws, https→wss
	return &Directory{db, publicURL, relayURL, validator.New(), time.Now}, nil
}

// IsLocal reports whether pubkey holds a handle on this instance. Fails closed.
func (d *Directory) IsLocal(pubkey nostr.PubKey) bool {
	local := false
	err := d.db.View(func(tx *bbolt.Tx) error {
		local = tx.Bucket(byPubKey).Get(pubkey[:]) != nil
		return nil
	})
	if err != nil {
		log.Printf("directory: IsLocal: %v", err)
	}
	return local && err == nil
}

// lookup returns the key holding name, if any.
func (d *Directory) lookup(name string) (pubkey nostr.PubKey, found bool) {
	_ = d.db.View(func(tx *bbolt.Tx) error {
		if v := tx.Bucket(byName).Get([]byte(name)); len(v) == 32 {
			pubkey, found = nostr.PubKey(v), true
		}
		return nil
	})
	return pubkey, found
}

// put claims name for pubkey: first come, first served, one handle per key.
func (d *Directory) put(name string, pubkey nostr.PubKey) error {
	return d.db.Update(func(tx *bbolt.Tx) error {
		names, keys := tx.Bucket(byName), tx.Bucket(byPubKey)
		if names.Get([]byte(name)) != nil || keys.Get(pubkey[:]) != nil {
			return errTaken
		}
		if err := names.Put([]byte(name), pubkey[:]); err != nil {
			return err
		}
		return keys.Put(pubkey[:], []byte(name))
	})
}

// RelayURL is this instance's relay, wss://host (ws:// for http).
func (d *Directory) RelayURL() string { return d.relayURL }

func (d *Directory) Register(mux *http.ServeMux) {
	mux.HandleFunc("GET /.well-known/nostr.json", d.nostrJSON)
	mux.HandleFunc("POST "+ClaimPath, d.claim)
}

type nip05Response struct {
	Names  map[string]string   `json:"names"`
	Relays map[string][]string `json:"relays"`
}

func (d *Directory) nostrJSON(w http.ResponseWriter, r *http.Request) {
	name := strings.ToLower(r.URL.Query().Get("name"))
	resp := nip05Response{Names: map[string]string{}, Relays: map[string][]string{}}
	if pubkey, ok := d.lookup(name); ok {
		resp.Names[name] = pubkey.Hex()
		resp.Relays[pubkey.Hex()] = []string{d.relayURL}
	}
	w.Header().Set("Access-Control-Allow-Origin", "*") // required by NIP-05
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(resp)
}

type claimRequest struct {
	Name string `json:"name" validate:"required"`
}

type claimResponse struct {
	Name   string `json:"name"`
	PubKey string `json:"pubkey"`
}

func (d *Directory) claim(w http.ResponseWriter, r *http.Request) {
	body, err := io.ReadAll(io.LimitReader(r.Body, 1024))
	if err != nil {
		handlers.HandleError(w, dto.ErrInvalidInput("Unreadable body", err))
		return
	}
	pubkey, err := nip98.Verify(r, d.publicURL+ClaimPath, body, d.now())
	if err != nil {
		handlers.HandleError(w, dto.ErrUnauthorized("Invalid NIP-98 authorization", err))
		return
	}

	r.Body = io.NopCloser(bytes.NewReader(body))
	in, ok := handlers.ReadAndValidate[claimRequest](w, r, d.validate)
	if !ok {
		return
	}
	name := strings.ToLower(in.Name)
	if !nameRe.MatchString(name) {
		handlers.HandleError(w, dto.ErrInvalidInput("Name must be 3-32 chars of a-z 0-9 . _ -", nil))
		return
	}

	err = d.put(name, pubkey)
	if errors.Is(err, errTaken) {
		handlers.HandleError(w, dto.ErrConflict("Name taken, or this key already has a handle", err))
		return
	}
	if err != nil {
		handlers.HandleError(w, err)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusCreated)
	_ = json.NewEncoder(w).Encode(claimResponse{name, pubkey.Hex()})
}
