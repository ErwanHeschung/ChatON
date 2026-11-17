// Package configloader provides functionality to load and validate
// application configuration using Viper.
package configloader

import (
	"log"
	"reflect"
	"strings"

	"github.com/joho/godotenv"
	"github.com/spf13/viper"
)

type Config struct {
	Server ServerConfig
}

type ServerConfig struct {
	Port             string   // SERVER_PORT, e.g. ":8080"
	AllowedOrigin    string   // SERVER_ALLOWEDORIGIN, browser CORS (dev only, Tauri goes through core)
	PublicURL        string   // SERVER_PUBLICURL, e.g. "https://chaton.example" (NIP-98 "u", NIP-05 relays)
	DBPath           string   // SERVER_DBPATH, bbolt file (events + handles)
	Name             string   // SERVER_NAME, shown in NIP-11
	BlockedPubkeys   []string // SERVER_BLOCKEDPUBKEYS, comma-separated hex keys
	RelayKey         string   // SERVER_RELAYKEY, hex: signs group state; default relay.key next to the DB
	BlobDir          string   // SERVER_BLOBDIR, uploaded files; default blobs/ next to the DB
	VAPIDKey         string   // SERVER_VAPIDKEY, base64url P-256: signs pushes; default vapid.key next to the DB
	PushAllowPrivate bool     // SERVER_PUSHALLOWPRIVATE, dev only: push to private IPs and plain http
	LiveKitURL       string   // SERVER_LIVEKITURL, wss:// address clients join; voice is off when unset
	LiveKitKey       string   // SERVER_LIVEKITKEY, LiveKit API key
	LiveKitSecret    string   // SERVER_LIVEKITSECRET, LiveKit API secret: signs tokens, checks webhooks
}

func LoadConfig() (*Config, error) {
	if err := godotenv.Load(); err != nil {
		log.Println("Info: .env file not found, proceeding with system environment variables")
	}

	v := viper.New()

	v.AutomaticEnv()
	v.SetEnvKeyReplacer(strings.NewReplacer(".", "_"))

	var cfg Config
	bindAllEnv(v, cfg)

	if err := v.Unmarshal(&cfg); err != nil {
		return nil, err
	}

	return &cfg, nil
}

func bindAllEnv(v *viper.Viper, i interface{}, parts ...string) {
	ift := reflect.TypeOf(i)

	for n := 0; n < ift.NumField(); n++ {
		field := ift.Field(n)
		tag := field.Tag.Get("mapstructure")

		if tag == "" {
			tag = strings.ToLower(field.Name)
		}

		path := append(parts, tag)
		if field.Type.Kind() == reflect.Struct {
			bindAllEnv(v, reflect.New(field.Type).Elem().Interface(), path...)
		} else {
			v.SetDefault(strings.Join(path, "."), nil)
		}
	}
}
