package naming

import "testing"

func TestParseSeasonEpisode(t *testing.T) {
	cases := []struct {
		in         string
		wantSeason int
		wantEp     int
	}{
		{"KLOVN S01", 1, 0},
		{"KLOVN - S02E03 - Pilot", 2, 3},
		{"KLOVN - Ep 4", 0, 4},
		{"Season 2", 2, 0},
		{"MASK", 0, 0},
		{"", 0, 0},
	}
	for _, c := range cases {
		s, e := ParseSeasonEpisode(c.in)
		if s != c.wantSeason || e != c.wantEp {
			t.Errorf("ParseSeasonEpisode(%q) = (%d,%d), want (%d,%d)", c.in, s, e, c.wantSeason, c.wantEp)
		}
	}
}

func TestParseSeriesTitle(t *testing.T) {
	cases := []struct {
		in   string
		want string
	}{
		{"KLOVN S01", "KLOVN"},
		{"Breaking Bad - S01E01 - Pilot", "Breaking Bad"},
		{"Inception", "Inception"},
	}
	for _, c := range cases {
		got := ParseSeriesTitle(c.in)
		if got != c.want {
			t.Errorf("ParseSeriesTitle(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}
