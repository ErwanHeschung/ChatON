package main

import (
	"context"
	"log"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	"fiatjaf.com/nostr/eventstore/boltdb"

	"github.com/ErwanHeschung/ChatON/server/configs"
	"github.com/ErwanHeschung/ChatON/server/internal/community"
	"github.com/ErwanHeschung/ChatON/server/internal/configloader"
	"github.com/ErwanHeschung/ChatON/server/internal/directory"
	"github.com/ErwanHeschung/ChatON/server/internal/media"
	"github.com/ErwanHeschung/ChatON/server/internal/push"
	"github.com/ErwanHeschung/ChatON/server/internal/relay"
	"github.com/ErwanHeschung/ChatON/server/internal/rules"
	"github.com/ErwanHeschung/ChatON/server/internal/safehttp"
	"github.com/ErwanHeschung/ChatON/server/internal/voice"
)

func main() {
	cfg, err := configloader.LoadConfig()
	if err != nil {
		log.Fatal("Cannot load config:", err)
	}
	s := &cfg.Server
	if s.DBPath == "" {
		s.DBPath = "chaton.bolt"
	}
	if s.Port == "" {
		s.Port = ":8080"
	}
	if s.PublicURL == "" {
		s.PublicURL = "http://localhost" + s.Port
	}

	// One bbolt file: events (eventstore) and handles (directory buckets).
	db := &boltdb.BoltBackend{Path: s.DBPath}
	if err := db.Init(); err != nil {
		log.Fatal("Cannot open database:", err)
	}
	defer db.Close()

	dir, err := directory.New(db.DB, s.PublicURL)
	if err != nil {
		log.Fatal("Cannot init directory:", err)
	}
	// SERVER_RULES (rules.yaml next to the DB by default); SIGHUP reloads keys and instances.
	if s.Rules == "" {
		s.Rules = filepath.Join(filepath.Dir(s.DBPath), "rules.yaml")
	}
	instanceRules, err := rules.Open(s.Rules, s.BlockedPubkeys)
	if err != nil {
		log.Fatal("Cannot load rules: ", err)
	}
	hup := make(chan os.Signal, 1)
	signal.Notify(hup, syscall.SIGHUP)
	go func() {
		for range hup {
			if err := instanceRules.Reload(); err != nil {
				log.Println("rules not reloaded:", err)
			} else {
				log.Println("rules reloaded")
			}
		}
	}()
	allowPrivate := s.PushAllowPrivate // dev: NIP-05 checks of local instances too
	key, err := community.LoadKey(s.RelayKey, filepath.Join(filepath.Dir(s.DBPath), "relay.key"))
	if err != nil {
		log.Fatal("Cannot load relay key:", err)
	}
	groups, err := community.New(key, dir.RelayURL(), db, dir)
	if err != nil {
		log.Fatal("Cannot load communities:", err)
	}
	rl := relay.New(relay.Config{
		Name:        s.Name,
		Description: "ChatON instance",
		Rules:       instanceRules,
		Countries:   s.Country,
	}, db, dir, groups)
	groups.UseRules(instanceRules, rules.Verifier{Client: safehttp.Client(allowPrivate, 5*time.Second)})

	mux := rl.Router() // khatru serves WebSocket + NIP-11 on "/", the rest falls through here
	dir.Register(mux)
	instanceRules.Register(mux)

	// Push (spec/federation.md): wakes local users' phones when a DM arrives.
	vapid, err := push.LoadVAPIDKey(s.VAPIDKey, filepath.Join(filepath.Dir(s.DBPath), "vapid.key"))
	if err != nil {
		log.Fatal("Cannot load VAPID key:", err)
	}
	if s.PushAllowPrivate {
		log.Println("SERVER_PUSHALLOWPRIVATE: pushes may go to private addresses (dev only)")
	}
	pusher, err := push.New(db.DB, dir, s.PublicURL, vapid, s.PushAllowPrivate)
	if err != nil {
		log.Fatal("Cannot init push:", err)
	}
	pusher.Bind(rl)
	pusher.Register(mux)

	// Voice (spec § 11): only next to a LiveKit server.
	lk := voice.Config{URL: s.LiveKitURL, Key: s.LiveKitKey, Secret: s.LiveKitSecret}
	switch {
	case lk == voice.Config{}:
		log.Println("Voice off: set SERVER_LIVEKITURL, SERVER_LIVEKITKEY and SERVER_LIVEKITSECRET")
	case lk.URL == "" || lk.Key == "" || lk.Secret == "":
		log.Fatal("Voice: set all of SERVER_LIVEKITURL, SERVER_LIVEKITKEY and SERVER_LIVEKITSECRET, or none")
	default:
		v := voice.New(lk, groups, dir, s.PublicURL)
		v.Bind(rl)
		v.Register(mux)
		go v.Reconcile(context.Background())
	}

	// Files (spec § 10): own index store, never the relay's.
	dataDir := filepath.Dir(s.DBPath)
	if s.BlobDir == "" {
		s.BlobDir = filepath.Join(dataDir, "blobs")
	}
	index := &boltdb.BoltBackend{Path: filepath.Join(dataDir, "blobs.bolt")}
	if err := index.Init(); err != nil {
		log.Fatal("Cannot open the blob index:", err)
	}
	defer index.Close()
	if _, err := media.New(rl, strings.TrimSuffix(s.PublicURL, "/"), dir, s.BlobDir, index); err != nil {
		log.Fatal("Cannot start file storage:", err)
	}
	mux.HandleFunc("GET /health", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"status": "UP"}`))
	})

	log.Printf("ChatON %s on %s (db %s)", s.PublicURL, s.Port, s.DBPath)
	if err := http.ListenAndServe(s.Port, configs.SetupCORS(cfg, media.Guard(rl))); err != nil {
		log.Fatal(err)
	}
}
