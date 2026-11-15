// Command genvectors writes spec/test-vectors/events.json: NIP-01 events with
// deterministic BIP-340 signatures (aux_rand = 32 zero bytes). core (Rust) and
// server (Go) tests must both reproduce every serialized/id/sig byte for byte.
//
//	go run ./cmd/genvectors > ../spec/test-vectors/events.json
package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"

	"github.com/btcsuite/btcd/btcec/v2"
	"github.com/btcsuite/btcd/btcec/v2/schnorr"
	"github.com/nbd-wtf/go-nostr"
)

const createdAt = 1700000000

var (
	aux   [32]byte // all zeros: makes signatures reproducible across implementations
	alice = key(1)
	bob   = key(2)
	ephem = key(3) // gift wrap throwaway key
	v0    = nostr.Tag{"v", "0"}
)

type Unsigned struct {
	PubKey    string     `json:"pubkey"`
	CreatedAt int64      `json:"created_at"`
	Kind      int        `json:"kind"`
	Tags      nostr.Tags `json:"tags"`
	Content   string     `json:"content"`
}

type Signed struct {
	ID string `json:"id"`
	Unsigned
	Sig string `json:"sig,omitempty"`
}

type Valid struct {
	Name       string   `json:"name"`
	PrivKey    string   `json:"privkey"`
	Event      Unsigned `json:"event"`
	Serialized string   `json:"serialized"`
	ID         string   `json:"id"`
	Sig        string   `json:"sig,omitempty"` // absent for kind 14 rumors
}

type Invalid struct {
	Name   string `json:"name"`
	Event  Signed `json:"event"`
	Reason string `json:"reason"` // bad_id | bad_sig | signed_rumor
}

type Vectors struct {
	Version int       `json:"version"`
	AuxRand string    `json:"aux_rand"`
	Valid   []Valid   `json:"valid"`
	Invalid []Invalid `json:"invalid"`
}

type keypair struct{ sk, pk string }

func key(n byte) keypair {
	b := make([]byte, 32)
	b[31] = n
	_, pub := btcec.PrivKeyFromBytes(b)
	return keypair{hex.EncodeToString(b), hex.EncodeToString(schnorr.SerializePubKey(pub))}
}

// sign returns the event id, and the BIP-340 signature unless rumor is true.
func sign(k keypair, e Unsigned, rumor bool) (serialized, id, sig string) {
	ev := nostr.Event{PubKey: e.PubKey, CreatedAt: nostr.Timestamp(e.CreatedAt), Kind: e.Kind, Tags: e.Tags, Content: e.Content}
	ser := ev.Serialize()
	h := sha256.Sum256(ser)
	if rumor {
		return string(ser), hex.EncodeToString(h[:]), ""
	}
	skb, _ := hex.DecodeString(k.sk)
	priv, _ := btcec.PrivKeyFromBytes(skb)
	s, err := schnorr.Sign(priv, h[:], schnorr.CustomNonce(aux))
	if err != nil {
		panic(err)
	}
	return string(ser), hex.EncodeToString(h[:]), hex.EncodeToString(s.Serialize())
}

func valid(name string, k keypair, kind int, tags nostr.Tags, content string) Valid {
	e := Unsigned{k.pk, createdAt, kind, tags, content}
	ser, id, sig := sign(k, e, kind == 14)
	return Valid{name, k.sk, e, ser, id, sig}
}

func generate() []byte {
	payload := sha256.Sum256([]byte(`{"name":"alice"}`))
	// Seal/gift wrap content is NIP-44 ciphertext: opaque here, NIP-44 has its own vectors.
	const opaque = "AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"

	vs := Vectors{Version: 0, AuxRand: hex.EncodeToString(aux[:])}
	vs.Valid = []Valid{
		valid("profile", alice, 0, nostr.Tags{v0}, `{"name":"alice","display_name":"Alice","nip05":"alice@chaton.example"}`),
		valid("relay_list", alice, 10002, nostr.Tags{v0, {"r", "wss://relay.chaton.example"}, {"r", "wss://read.chaton.example", "read"}}, ""),
		valid("dm_relay_list", alice, 10050, nostr.Tags{v0, {"relay", "wss://dm.chaton.example"}}, ""),
		valid("dm_rumor", alice, 14, nostr.Tags{v0, {"p", bob.pk}}, "Salut Bob 👋"),
		valid("seal", alice, 13, nostr.Tags{}, opaque),
		valid("gift_wrap", ephem, 1059, nostr.Tags{{"p", bob.pk}}, opaque),
		valid("relay_auth", alice, 22242, nostr.Tags{v0, {"relay", "wss://relay.chaton.example"}, {"challenge", "c0ffee"}}, ""),
		valid("http_auth", alice, 27235, nostr.Tags{v0, {"u", "https://chaton.example/api/handle"}, {"method", "POST"}, {"payload", hex.EncodeToString(payload[:])}}, ""),
		// serialization edge cases (rumors: id only)
		valid("escapes", alice, 14, nostr.Tags{v0}, "quote \" backslash \\ newline \n tab \t cr \r bs \b ff \f"),
		valid("unicode_raw", alice, 14, nostr.Tags{v0}, "émoji 🎉 日本語 ñ"),
		valid("no_html_escape", alice, 14, nostr.Tags{v0}, "</script> & <b>     /"),
		valid("empty_content_no_tags", alice, 14, nostr.Tags{}, ""),
		valid("tags_escaped", alice, 14, nostr.Tags{v0, {"p", bob.pk, "wss://r.chaton.example", "bob"}, {"t", "quote\"tag\\n"}}, "x"),
	}

	// bad_sig: content changed after signing, id recomputed, old sig kept.
	p := vs.Valid[0]
	badSig := Signed{Unsigned: p.Event, Sig: p.Sig}
	badSig.Content = `{"name":"mallory"}`
	_, badSig.ID, _ = sign(alice, badSig.Unsigned, true)

	// bad_id: last hex digit of the id changed.
	last := "0"
	if p.ID[63] == '0' {
		last = "1"
	}
	badID := Signed{ID: p.ID[:63] + last, Unsigned: p.Event, Sig: p.Sig}

	// signed_rumor: a kind 14 that carries a valid signature.
	r := vs.Valid[3]
	_, rid, rsig := sign(alice, r.Event, false)

	vs.Invalid = []Invalid{
		{"content_tampered", badSig, "bad_sig"},
		{"id_mismatch", badID, "bad_id"},
		{"signed_rumor", Signed{ID: rid, Unsigned: r.Event, Sig: rsig}, "signed_rumor"},
	}

	// One vector per line: readable diffs, grep-friendly.
	var b bytes.Buffer
	fmt.Fprintf(&b, "{\n  \"version\": %d,\n  \"aux_rand\": %q,\n", vs.Version, vs.AuxRand)
	writeList(&b, "valid", vs.Valid, ",")
	writeList(&b, "invalid", vs.Invalid, "")
	b.WriteString("}\n")
	return b.Bytes()
}

func writeList[T any](b *bytes.Buffer, name string, items []T, trailer string) {
	fmt.Fprintf(b, "  %q: [\n", name)
	for i, it := range items {
		var line bytes.Buffer
		enc := json.NewEncoder(&line)
		enc.SetEscapeHTML(false)
		if err := enc.Encode(it); err != nil {
			panic(err)
		}
		b.WriteString("    ")
		b.Write(bytes.TrimRight(line.Bytes(), "\n"))
		if i < len(items)-1 {
			b.WriteByte(',')
		}
		b.WriteByte('\n')
	}
	fmt.Fprintf(b, "  ]%s\n", trailer)
}

func main() {
	_, _ = os.Stdout.Write(generate())
}
