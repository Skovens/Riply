package main

import "testing"

// Security-relevant: main only prints the no-auth warning for non-loopback
// binds, and the warning gates the "do not expose this address" advice.
func TestIsLoopbackHost(t *testing.T) {
	cases := []struct {
		host string
		want bool
	}{
		{"", false},
		{"127.0.0.1", true},
		{"127.8.9.10", true}, // whole 127/8 is loopback
		{"::1", true},
		{"::ffff:127.0.0.1", true}, // IPv4-mapped loopback
		{"0.0.0.0", false},
		{"::", false},
		{"192.168.1.50", false},
		{"localhost", true}, // resolves loopback on any sane host
		{"riply-no-such-host.invalid", false},
	}
	for _, c := range cases {
		if got := isLoopbackHost(c.host); got != c.want {
			t.Errorf("isLoopbackHost(%q) = %v, want %v", c.host, got, c.want)
		}
	}
}
