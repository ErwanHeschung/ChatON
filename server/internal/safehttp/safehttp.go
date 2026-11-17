// Package safehttp: an HTTP client that only reaches public addresses (no SSRF
// through push endpoints or NIP-05 lookups of other instances).
package safehttp

import (
	"fmt"
	"net"
	"net/http"
	"net/netip"
	"syscall"
	"time"
)

var cgnat = netip.MustParsePrefix("100.64.0.0/10")

// Public: a global unicast address, not private nor carrier-grade NAT.
func Public(a netip.Addr) bool {
	a = a.Unmap()
	return a.IsGlobalUnicast() && !a.IsPrivate() && !cgnat.Contains(a)
}

// Client refuses to connect to non-public addresses (unless allowPrivate, dev
// only) and doesn't follow redirects.
func Client(allowPrivate bool, timeout time.Duration) *http.Client {
	dialer := &net.Dialer{
		Timeout: 10 * time.Second,
		Control: func(_, address string, _ syscall.RawConn) error {
			if allowPrivate {
				return nil
			}
			ap, err := netip.ParseAddrPort(address)
			if err != nil || !Public(ap.Addr()) {
				return fmt.Errorf("%s is not a public address", address)
			}
			return nil
		},
	}
	return &http.Client{
		Timeout:       timeout,
		Transport:     &http.Transport{DialContext: dialer.DialContext, TLSHandshakeTimeout: 10 * time.Second},
		CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
	}
}
