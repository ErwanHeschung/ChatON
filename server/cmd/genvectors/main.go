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
	"strconv"
	"strings"

	"fiatjaf.com/nostr"
	"github.com/btcsuite/btcd/btcec/v2"
	"github.com/btcsuite/btcd/btcec/v2/schnorr"
)

const createdAt = 1700000000

var (
	aux   [32]byte // all zeros: makes signatures reproducible across implementations
	alice = key(1)
	bob   = key(2)
	ephem = key(3) // gift wrap throwaway key
	relay = key(4) // relay key (NIP-11 self): signs group state (39000+)
	carol = key(5)
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

// serialize is NIP-01 / spec § 1 written out, independent of any library:
// fiatjaf.com/nostr doesn't escape U+0008 and U+000C, the spec does.
func serialize(e Unsigned) []byte {
	b := fmt.Appendf(nil, `[0,"%s",%s,%s,[`, e.PubKey, strconv.FormatInt(e.CreatedAt, 10), strconv.Itoa(e.Kind))
	for i, tag := range e.Tags {
		if i > 0 {
			b = append(b, ',')
		}
		b = append(b, '[')
		for j, v := range tag {
			if j > 0 {
				b = append(b, ',')
			}
			b = appendString(b, v)
		}
		b = append(b, ']')
	}
	b = append(b, "],"...)
	b = appendString(b, e.Content)
	return append(b, ']')
}

func appendString(b []byte, s string) []byte {
	b = append(b, '"')
	for i := 0; i < len(s); i++ { // byte loop: UTF-8 continuation bytes are >= 0x80, copied as is
		switch c := s[i]; c {
		case '"':
			b = append(b, `\"`...)
		case '\\':
			b = append(b, `\\`...)
		case '\n':
			b = append(b, `\n`...)
		case '\r':
			b = append(b, `\r`...)
		case '\t':
			b = append(b, `\t`...)
		case '\b':
			b = append(b, `\b`...)
		case '\f':
			b = append(b, `\f`...)
		default:
			b = append(b, c)
		}
	}
	return append(b, '"')
}

// sign returns the event id, and the BIP-340 signature unless rumor is true.
func sign(k keypair, e Unsigned, rumor bool) (serialized, id, sig string) {
	ser := serialize(e)
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
	ser, id, sig := sign(k, e, kind == 14 || kind == 15)
	return Valid{name, k.sk, e, ser, id, sig}
}

func generate() []byte {
	payload := sha256.Sum256([]byte(`{"name":"alice"}`))
	channel := sha256.Sum256([]byte("chaton-link-v0|alice@chaton.example|K7F4"))
	const community, channel1, voice = "k3j9x2m8q0aa", "g7w2p5n1c4bb", "v8t3r6m2d5cc"
	// Call (spec § 11): fixed id and key, the E2EE itself is LiveKit's.
	callID, callKey := strings.Repeat("0f", 16), strings.Repeat("ef", 32)
	// Seal/gift wrap content is NIP-44 ciphertext: opaque here, NIP-44 has its own vectors.
	const opaque = "AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
	// Attachment fields: fixed values, the AES-GCM itself is tested in core.
	cipherHash, plainHash := sha256.Sum256([]byte("ciphertext")), sha256.Sum256([]byte("plaintext"))
	fileX, fileOX := hex.EncodeToString(cipherHash[:]), hex.EncodeToString(plainHash[:])
	fileKey, fileNonce := strings.Repeat("ab", 32), strings.Repeat("cd", 12)

	vs := Vectors{Version: 0, AuxRand: hex.EncodeToString(aux[:])}
	vs.Valid = []Valid{
		valid("profile", alice, 0, nostr.Tags{v0}, `{"name":"alice","display_name":"Alice","nip05":"alice@chaton.example"}`),
		valid("relay_list", alice, 10002, nostr.Tags{v0, {"r", "wss://relay.chaton.example"}, {"r", "wss://read.chaton.example", "read"}}, ""),
		valid("dm_relay_list", alice, 10050, nostr.Tags{v0, {"relay", "wss://dm.chaton.example"}}, ""),
		valid("dm_rumor", alice, 14, nostr.Tags{v0, {"p", bob.pk}}, "Salut Bob 👋"),
		valid("seal", alice, 13, nostr.Tags{}, opaque),
		valid("gift_wrap", ephem, 1059, nostr.Tags{{"p", bob.pk}}, opaque),
		valid("relay_auth", alice, 22242, nostr.Tags{v0, {"relay", "wss://relay.chaton.example"}, {"challenge", "c0ffee"}}, ""),
		// device linking (spec § 8): ephemeral, signed by a throwaway key, channel = sha256 hex
		valid("device_link", ephem, 24135, nostr.Tags{v0, {"c", hex.EncodeToString(channel[:])}}, `{"type":"pake","msg":"AAAA"}`),
		valid("http_auth", alice, 27235, nostr.Tags{v0, {"u", "https://chaton.example/api/handle"}, {"method", "POST"}, {"payload", hex.EncodeToString(payload[:])}}, ""),
		// communities (spec § 9): C = community, S = channel
		valid("group_create", alice, 9007, nostr.Tags{v0, {"h", community}}, ""),
		valid("group_edit_metadata", alice, 9002, nostr.Tags{v0, {"h", community}, {"name", "Les chatons"}, {"about", "Pour les chats"},
			{"child", channel1}, {"owner", alice.pk},
			{"role", "admin", "Admin", "", "administrator"},
			{"role", "gardien", "Gardien", "#e5a50a", "hoist", "kick_members"},
			{"role", "everyone", "@everyone", "", "view_channel", "send_messages"}}, ""),
		valid("group_put_user", relay, 9000, nostr.Tags{v0, {"h", community}, {"p", alice.pk, "admin"}}, ""),
		valid("group_ban", alice, 9001, nostr.Tags{v0, {"h", community}, {"p", carol.pk}, {"ban"}}, "Spam"),
		valid("group_unban", alice, 9005, nostr.Tags{v0, {"h", community}, {"e", strings.Repeat("ab", 32)}, {"k", "9001"}, {"p", carol.pk}}, ""),
		valid("group_timeout", alice, 9000, nostr.Tags{v0, {"h", community}, {"p", carol.pk}, {"timeout", "1767225600"}}, "On se calme"),
		valid("group_create_invite", alice, 9009, nostr.Tags{v0, {"h", community}, {"code", "AB12CD34"}}, ""),
		valid("group_create_invite_limited", alice, 9009, nostr.Tags{v0, {"h", community}, {"code", "EF34GH56"}, {"expiration", "1767225600"}, {"max_uses", "10"}}, ""),
		valid("group_revoke_invite", alice, 9005, nostr.Tags{v0, {"h", community}, {"e", strings.Repeat("cd", 32)}, {"k", "9009"}}, ""),
		valid("group_join_request", bob, 9021, nostr.Tags{v0, {"h", community}, {"code", "AB12CD34"}}, ""),
		valid("group_join_request_nip05", bob, 9021, nostr.Tags{v0, {"h", community}, {"code", "AB12CD34"}, {"nip05", "bob@ronron.example"}}, ""),
		valid("group_chat_message", bob, 9, nostr.Tags{v0, {"h", channel1}}, "Bonjour tout le monde"),
		valid("group_metadata", relay, 39000, nostr.Tags{v0, {"d", community}, {"name", "Les chatons"}, {"private"}, {"restricted"}, {"closed"}, {"child", channel1}}, ""),
		valid("groups_list", bob, 10009, nostr.Tags{v0, {"group", community, "wss://chaton.example"}}, ""),
		// moderation and state without vectors before step 12, then roles and owner (spec § 9)
		valid("group_remove_user", alice, 9001, nostr.Tags{v0, {"h", community}, {"p", bob.pk}}, ""),
		valid("group_delete_event", bob, 9005, nostr.Tags{v0, {"h", channel1}, {"e", strings.Repeat("ab", 32)}}, ""),
		valid("group_delete_group", alice, 9008, nostr.Tags{v0, {"h", channel1}}, ""),
		valid("group_leave_request", bob, 9022, nostr.Tags{v0, {"h", community}}, ""),
		valid("group_admins", relay, 39001, nostr.Tags{v0, {"d", community}, {"p", alice.pk, "admin"}}, ""),
		valid("group_members", relay, 39002, nostr.Tags{v0, {"d", community}, {"p", alice.pk}, {"p", bob.pk}}, ""),
		valid("group_metadata_owner", relay, 39000, nostr.Tags{v0, {"d", community}, {"name", "Les chatons"}, {"private"}, {"restricted"}, {"closed"},
			{"child", channel1}, {"owner", alice.pk}}, ""),
		valid("group_roles", relay, 39003, nostr.Tags{v0, {"d", community},
			{"role", "admin", "Admin", "", "administrator"},
			{"role", "moderator", "Modérateur", "", "manage_messages", "kick_members", "ban_members", "moderate_members",
				"create_invite", "view_audit_log", "mute_members", "move_members"},
			{"role", "gardien", "Gardien", "#e5a50a", "hoist", "mentionable", "kick_members"},
			{"role", "everyone", "@everyone", "", "view_channel", "send_messages", "attach_files", "add_reactions", "connect", "speak", "stream"}}, ""),
		// files (spec § 10): DM attachment (rumor, id only) and channel attachment
		valid("dm_file_rumor", alice, 15, nostr.Tags{v0, {"p", bob.pk}, {"file-type", "image/png"}, {"encryption-algorithm", "aes-gcm"},
			{"decryption-key", fileKey}, {"decryption-nonce", fileNonce}, {"x", fileX}, {"ox", fileOX}, {"size", "1234"}, {"name", "chat.png"}},
			"https://chaton.example/"+fileX),
		// group DM (spec § 5): room = author + every p; naming event = empty content + subject
		valid("group_dm_rumor", alice, 14, nostr.Tags{v0, {"p", bob.pk}, {"p", carol.pk}, {"subject", "Projet"}}, ""),
		valid("group_file_message", bob, 9, nostr.Tags{v0, {"h", channel1}, {"imeta", "url https://chaton.example/" + fileX, "m image/png",
			"x " + fileX, "ox " + fileOX, "size 1234", "name chat.png", "encryption-algorithm aes-gcm",
			"decryption-key " + fileKey, "decryption-nonce " + fileNonce}}, "Regardez !"),
		// voice (spec § 11): voice channel, its participants (relay), DM call and its end (rumors)
		valid("voice_channel_edit_metadata", alice, 9002, nostr.Tags{v0, {"h", voice}, {"name", "Vocal"}, {"parent", community}, {"livekit"}}, ""),
		valid("group_voice_participants", relay, 39004, nostr.Tags{v0, {"d", voice}, {"participant", alice.pk}, {"participant", bob.pk}}, ""),
		valid("group_voice_participants_moderated", relay, 39004, nostr.Tags{v0, {"d", voice}, {"participant", alice.pk}, {"participant", bob.pk}, {"muted", bob.pk}, {"deafened", bob.pk}}, ""),
		// channels and categories (spec § 9): one 9007 per channel, layout in the community's 9002
		valid("channel_create", bob, 9007, nostr.Tags{v0, {"h", channel1}, {"parent", community}, {"name", "général"}, {"category", "texte"}}, ""),
		valid("channel_create_voice", bob, 9007, nostr.Tags{v0, {"h", voice}, {"parent", community}, {"name", "Vocal"}, {"livekit"}}, ""),
		valid("channel_edit_settings", alice, 9002, nostr.Tags{v0, {"h", channel1}, {"name", "général"}, {"about", "On parle de chats"},
			{"parent", community}, {"slowmode", "10"}}, ""),
		valid("voice_channel_edit_limit", alice, 9002, nostr.Tags{v0, {"h", voice}, {"name", "Vocal"}, {"parent", community}, {"livekit"}, {"user_limit", "5"}}, ""),
		valid("channel_private", alice, 9002, nostr.Tags{v0, {"h", channel1}, {"name", "staff"}, {"parent", community},
			{"deny", "everyone", "view_channel"}, {"allow", "moderator", "view_channel"}, {"allow", bob.pk, "view_channel", "send_messages"}}, ""),
		valid("community_layout", relay, 39000, nostr.Tags{v0, {"d", community}, {"name", "Les chatons"}, {"private"}, {"restricted"}, {"closed"},
			{"child", channel1, "texte"}, {"child", voice, "vocal"}, {"owner", alice.pk}, {"category", "texte", "Salons textuels"}, {"category", "vocal", "Salons vocaux"}}, ""),
		valid("call_rumor", alice, 14, nostr.Tags{v0, {"p", bob.pk}, {"call", callID, "https://chaton.example", callKey}}, "📞 Appel vocal"),
		valid("call_end_rumor", alice, 14, nostr.Tags{v0, {"p", bob.pk}, {"call-end", callID}}, "Appel terminé"),
		// federation (spec § 5): the author's handle in a rumor, my private blocks
		valid("dm_rumor_nip05", alice, 14, nostr.Tags{v0, {"p", bob.pk}, {"nip05", "alice@chaton.example"}}, "Salut Bob 👋"),
		valid("mute_list", alice, 10000, nostr.Tags{v0}, opaque),
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
