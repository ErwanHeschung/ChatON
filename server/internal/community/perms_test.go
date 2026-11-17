package community

import (
	"encoding/json"
	"os"
	"slices"
	"testing"

	"fiatjaf.com/nostr"
)

type permVectors struct {
	Permissions []string   `json:"permissions"`
	Owner       string     `json:"owner"`
	Roles       nostr.Tags `json:"roles"`
	Effective   []struct {
		Name    string     `json:"name"`
		Member  string     `json:"member"`
		Roles   []string   `json:"roles"`
		Timeout int64      `json:"timeout"`
		Now     int64      `json:"now"`
		Channel nostr.Tags `json:"channel"`
		Expect  []string   `json:"expect"`
	} `json:"effective"`
	Outranks []struct {
		Name string `json:"name"`
		A    struct {
			PubKey string   `json:"pubkey"`
			Roles  []string `json:"roles"`
		} `json:"a"`
		B struct {
			PubKey string   `json:"pubkey"`
			Roles  []string `json:"roles"`
		} `json:"b"`
		Expect bool `json:"expect"`
	} `json:"outranks"`
}

// spec/test-vectors/permissions.json: the same cases core (Rust) runs.
func TestPermissionVectors(t *testing.T) {
	raw, err := os.ReadFile("../../../spec/test-vectors/permissions.json")
	if err != nil {
		t.Fatal(err)
	}
	var v permVectors
	if err := json.Unmarshal(raw, &v); err != nil {
		t.Fatal(err)
	}
	if !slices.Equal(v.Permissions, Permissions) {
		t.Fatalf("permissions: %v, want %v", Permissions, v.Permissions)
	}
	policy := Policy{Owner: nostr.MustPubKeyFromHex(v.Owner), Roles: ParseRoles(v.Roles)}
	if len(policy.Roles) != len(v.Roles) {
		t.Fatalf("roles: parsed %d of %d", len(policy.Roles), len(v.Roles))
	}
	for _, c := range v.Effective {
		m := Member{PubKey: nostr.MustPubKeyFromHex(c.Member), Roles: c.Roles, Timeout: nostr.Timestamp(c.Timeout)}
		var channel []Override
		if c.Channel != nil {
			channel = ParseOverrides(c.Channel)
		}
		got := policy.Effective(m, channel, nostr.Timestamp(c.Now)).Names()
		if !slices.Equal(got, c.Expect) {
			t.Errorf("%s: %v, want %v", c.Name, got, c.Expect)
		}
	}
	for _, c := range v.Outranks {
		a := Member{PubKey: nostr.MustPubKeyFromHex(c.A.PubKey), Roles: c.A.Roles}
		b := Member{PubKey: nostr.MustPubKeyFromHex(c.B.PubKey), Roles: c.B.Roles}
		if got := policy.Outranks(a, b); got != c.Expect {
			t.Errorf("%s: %v, want %v", c.Name, got, c.Expect)
		}
	}
}
