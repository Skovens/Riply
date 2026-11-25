package rip

import "testing"

func sampleInfo(durations []string) string {
	out := "TCOUNT:" + itoa(len(durations)) + "\n"
	for i, d := range durations {
		out += "TINFO:" + itoa(i+1) + ",9,0,\"" + d + "\"\n"
		out += "TINFO:" + itoa(i+1) + ",2,0,\"1024\"\n"
	}
	return out
}

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	digits := ""
	for n > 0 {
		digits = string(rune('0'+n%10)) + digits
		n /= 10
	}
	return digits
}

func TestParseDiscInfoHashStableAndDistinct(t *testing.T) {
	a := parseDiscInfo(sampleInfo([]string{"2:10:16", "1:30:00"}))
	b := parseDiscInfo(sampleInfo([]string{"2:10:16", "1:30:00"}))
	c := parseDiscInfo(sampleInfo([]string{"2:10:16", "1:31:00"}))

	if a.Hash == "" {
		t.Fatal("hash empty, want computed from TINFO metadata")
	}
	if a.Hash != b.Hash {
		t.Errorf("same metadata produced different hashes: %s vs %s", a.Hash, b.Hash)
	}
	if a.Hash == c.Hash {
		t.Error("different metadata produced the same hash")
	}
	if len(a.Hash) != 64 {
		t.Errorf("hash length = %d, want 64 (sha256 hex)", len(a.Hash))
	}
}

func TestParseDiscInfoHashEmptyWithoutTINFO(t *testing.T) {
	info := parseDiscInfo("DRV:0,2,999,4,\"BD-RE\",\"DVD_VIDEO\",\"/dev/sr0\"")
	if info.Hash != "" {
		t.Errorf("hash = %q, want empty without TINFO metadata", info.Hash)
	}
	if info.Title != "DVD_VIDEO" {
		t.Errorf("title = %q, want DVD_VIDEO", info.Title)
	}
}
