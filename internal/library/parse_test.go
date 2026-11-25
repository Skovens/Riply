package library

import "testing"

func TestParseYearName(t *testing.T) {
	cases := []struct {
		in        string
		wantTitle string
		wantYear  int
	}{
		{"KLOVN (2010)", "KLOVN", 2010},
		{"Anna (1987).mkv", "Anna (1987).mkv", 0}, // not a trailing year: no match at end
		{"Just A Title", "Just A Title", 0},
		{"(2010)", "(2010)", 0}, // empty title falls back to the whole string
	}
	for _, c := range cases {
		title, year := ParseYearName(c.in)
		if title != c.wantTitle || year != c.wantYear {
			t.Errorf("ParseYearName(%q) = (%q, %d), want (%q, %d)", c.in, title, year, c.wantTitle, c.wantYear)
		}
	}
}

func TestParsePath(t *testing.T) {
	cases := []struct {
		in      string
		ok      bool
		mt      string
		title   string
		year    int
		season  int
		episode int
	}{
		{in: "KLOVN (2010)/Season 2/KLOVN - S02E03 - X.mkv", ok: true, mt: "tv", title: "KLOVN", year: 2010, season: 2, episode: 3},
		{in: "Inception (2010)/Inception.mkv", ok: true, mt: "movie", title: "Inception", year: 2010},
		{in: "Inception (2010)/Inception (2010).mkv", ok: true, mt: "movie", title: "Inception", year: 2010},
		{in: "Inception (2010)/Extras/Deleted.mkv", ok: false},
		{in: "Loose Movie (1999).mkv", ok: true, mt: "movie", title: "Loose Movie", year: 1999},
		{in: "notes.txt", ok: false},
		{in: ".hidden/Thing.mkv", ok: false},
	}
	for _, c := range cases {
		e, ok := ParsePath(c.in)
		if ok != c.ok {
			t.Errorf("ParsePath(%q) ok = %v, want %v", c.in, ok, c.ok)
			continue
		}
		if !ok {
			continue
		}
		if e.MediaType != c.mt || e.Title != c.title || e.Year != c.year || e.Season != c.season || e.Episode != c.episode {
			t.Errorf("ParsePath(%q) = %+v, want mt=%s title=%s year=%d season=%d ep=%d",
				c.in, e, c.mt, c.title, c.year, c.season, c.episode)
		}
		if e.Source != "scan" {
			t.Errorf("ParsePath(%q) source = %q, want scan", c.in, e.Source)
		}
	}
}
