package main

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"testing"

	"fiatjaf.com/nostr"
	"github.com/btcsuite/btcd/btcec/v2"
	"github.com/btcsuite/btcd/btcec/v2/schnorr"
)

const vectorsPath = "../../../spec/test-vectors/events.json"

// Official BIP-340 vector #0: proves our signing is BIP-340 with aux_rand,
// i.e. what Rust's sign_schnorr_with_aux_rand produces.
func TestBIP340Vector0(t *testing.T) {
	sk, _ := hex.DecodeString("0000000000000000000000000000000000000000000000000000000000000003")
	priv, _ := btcec.PrivKeyFromBytes(sk)
	sig, err := schnorr.Sign(priv, make([]byte, 32), schnorr.CustomNonce([32]byte{}))
	if err != nil {
		t.Fatal(err)
	}
	want := "e907831f80848d1069a5371b402410364bdf1c5f8307b0084c55f1ce2dca821525f66a4a85ea8b71e482a74f382d2ce5ebeee8fdb2172f477df4900d310536c0"
	if got := hex.EncodeToString(sig.Serialize()); got != want {
		t.Fatalf("got %s, want %s", got, want)
	}
}

func TestCommittedVectorsUpToDate(t *testing.T) {
	committed, err := os.ReadFile(vectorsPath)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(committed, generate()) {
		t.Fatal("spec/test-vectors/events.json is stale or edited: run `go run ./cmd/genvectors > ../spec/test-vectors/events.json`")
	}
}

// Verifies the committed file itself (not generate()), so a hand edit is caught.
func TestVectorsVerify(t *testing.T) {
	raw, err := os.ReadFile(vectorsPath)
	if err != nil {
		t.Fatal(err)
	}
	var vs Vectors
	if err := json.Unmarshal(raw, &vs); err != nil {
		t.Fatal(err)
	}
	toEvent := func(id string, u Unsigned, sig string) nostr.Event {
		ev := nostr.Event{
			ID:        nostr.MustIDFromHex(id),
			PubKey:    nostr.MustPubKeyFromHex(u.PubKey),
			CreatedAt: nostr.Timestamp(u.CreatedAt),
			Kind:      nostr.Kind(u.Kind),
			Tags:      u.Tags,
			Content:   u.Content,
		}
		if sig != "" {
			if _, err := hex.Decode(ev.Sig[:], []byte(sig)); err != nil {
				t.Fatalf("sig %q: %v", sig, err)
			}
		}
		return ev
	}

	for _, v := range vs.Valid {
		ev := toEvent(v.ID, v.Event, v.Sig)
		if got := string(serialize(v.Event)); got != v.Serialized {
			t.Errorf("%s: serialized mismatch\n got %s\nwant %s", v.Name, got, v.Serialized)
		}
		// The library must agree with the spec, except on U+0008/U+000C: the relay
		// rejects those (relay.rejectAmbiguousEscapes), so its deviation never matters.
		if !strings.ContainsAny(fmt.Sprint(v.Event.Content, v.Event.Tags), "\b\f") && !ev.CheckID() {
			t.Errorf("%s: library id mismatch", v.Name)
		}
		if (v.Event.Kind == 14) != (v.Sig == "") {
			t.Errorf("%s: kind 14 must have no sig, every other kind must have one", v.Name)
		}
		if v.Sig != "" && !ev.VerifySignature() {
			t.Errorf("%s: bad signature", v.Name)
		}
	}

	for _, v := range vs.Invalid {
		ev := toEvent(v.Event.ID, v.Event.Unsigned, v.Event.Sig)
		var reason string
		switch ok := ev.VerifySignature(); {
		case ev.Kind == 14 && v.Event.Sig != "":
			reason = "signed_rumor"
		case !ev.CheckID():
			reason = "bad_id"
		case !ok:
			reason = "bad_sig"
		}
		if reason != v.Reason {
			t.Errorf("%s: got reason %q, want %q", v.Name, reason, v.Reason)
		}
	}
}
