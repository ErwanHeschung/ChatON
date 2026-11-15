// Package media is the instance's Blossom server (spec/protocol.md § 10):
// local users upload files (attachments arrive already encrypted by the
// client), anyone fetches them by hash. Same port as the relay, outside it.
package media

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"net/http"
	"net/url"
	"os"
	"strconv"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/eventstore"
	"fiatjaf.com/nostr/khatru"
	"fiatjaf.com/nostr/khatru/blossom"
	nipb7 "fiatjaf.com/nostr/nipb7/blossom"
)

// MaxBlobSize bounds one file (spec § 10).
const MaxBlobSize = 25 << 20

type Directory interface {
	IsLocal(pubkey nostr.PubKey) bool
}

// New serves Blossom on rl's HTTP router. Blobs are files in blobDir; index
// ("who uploaded what") is its own store, never the relay's: relay clients
// could otherwise query it.
func New(rl *khatru.Relay, publicURL string, dir Directory, blobDir string, index eventstore.Store) (*blossom.BlossomServer, error) {
	if err := os.MkdirAll(blobDir, 0o700); err != nil {
		return nil, fmt.Errorf("blob dir: %w", err)
	}
	root, err := os.OpenRoot(blobDir)
	if err != nil {
		return nil, fmt.Errorf("blob dir: %w", err)
	}

	bs := blossom.New(rl, publicURL)
	bs.Store = blobIndex{blossom.EventStoreBlobIndexWrapper{Store: index, ServiceURL: publicURL}}

	// Files are named by hash only (the handler checks it's 64 hex chars).
	bs.StoreBlob = func(_ context.Context, sha256, _ string, body []byte) error {
		return root.WriteFile(sha256, body, 0o600)
	}
	// ponytail: whole file in memory (≤ 25 MiB): khatru never closes the reader,
	// so an *os.File would leak. Stream once khatru closes it.
	bs.LoadBlob = func(_ context.Context, sha256, _ string) (io.ReadSeeker, *url.URL, error) {
		b, err := root.ReadFile(sha256)
		if err != nil {
			return nil, nil, err
		}
		return bytes.NewReader(b), nil, nil
	}
	bs.DeleteBlob = func(_ context.Context, sha256, _ string) error {
		if err := root.Remove(sha256); err != nil && !errors.Is(err, fs.ErrNotExist) {
			return err
		}
		return nil
	}

	bs.RejectUpload = func(_ context.Context, auth *nostr.Event, size int, _ string) (bool, string, int) {
		switch {
		case !dir.IsLocal(auth.PubKey):
			return true, "claim a handle on this instance first", 403
		case size > MaxBlobSize:
			return true, "file too large", 413
		}
		return false, "", 0
	}
	bs.RejectList = func(_ context.Context, auth *nostr.Event, pubkey nostr.PubKey) (bool, string, int) {
		if auth == nil || auth.PubKey != pubkey {
			return true, "you can only list your own files", 403
		}
		return false, "", 0
	}
	// Delete: the index only removes the caller's own entry, and the file once nobody owns it.
	return bs, nil
}

// Guard runs before Blossom: khatru allocates the announced Content-Length
// before any check, and /mirror would fetch a remote URL for anyone.
func Guard(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/mirror", "/media", "/report":
			http.NotFound(w, r)
			return
		case "/upload":
			if r.Method == http.MethodPut {
				size, err := strconv.ParseInt(r.Header.Get("Content-Length"), 10, 64)
				if err != nil || size <= 0 {
					http.Error(w, "missing Content-Length", http.StatusLengthRequired)
					return
				}
				if size > MaxBlobSize {
					http.Error(w, "file too large", http.StatusRequestEntityTooLarge)
					return
				}
				r.Body = http.MaxBytesReader(w, r.Body, size)
			}
		}
		next.ServeHTTP(w, r)
	})
}

// blobIndex fixes khatru's index on bbolt: its Keep and Delete write while their
// own query's read transaction is still open, which deadlocks bbolt. Here the
// read is over before the write. Same records, so the rest of the wrapper works.
type blobIndex struct {
	blossom.EventStoreBlobIndexWrapper
}

func (b blobIndex) entries(sha256 string, pubkey nostr.PubKey) []nostr.ID {
	var ids []nostr.ID
	for ev := range b.Store.QueryEvents(nostr.Filter{
		Authors: []nostr.PubKey{pubkey},
		Kinds:   []nostr.Kind{24242},
		Tags:    nostr.TagMap{"x": {sha256}},
	}, 10) {
		ids = append(ids, ev.ID)
	}
	return ids
}

func (b blobIndex) Keep(_ context.Context, blob nipb7.BlobDescriptor, pubkey nostr.PubKey) error {
	if len(b.entries(blob.SHA256, pubkey)) > 0 {
		return nil
	}
	// The wrapper's record: tags in this order (it parses them by position).
	ev := nostr.Event{
		PubKey:    pubkey,
		Kind:      24242,
		Tags:      nostr.Tags{{"x", blob.SHA256}, {"type", blob.Type}, {"size", strconv.Itoa(blob.Size)}},
		CreatedAt: blob.Uploaded,
	}
	ev.ID = ev.GetID()
	return b.Store.SaveEvent(ev)
}

func (b blobIndex) Delete(_ context.Context, sha256 string, pubkey nostr.PubKey) error {
	for _, id := range b.entries(sha256, pubkey) {
		if err := b.Store.DeleteEvent(id); err != nil {
			return err
		}
	}
	return nil
}
