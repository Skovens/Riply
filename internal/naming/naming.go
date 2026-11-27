package naming

import (
	"regexp"
	"strconv"
	"strings"
)

// Result is one metadata match, provider-neutral: TMDb carries a numeric ID
// (ExtID = decimal ID), OMDb carries its imdbID in ExtID with ID unset.
type Result struct {
	ID       int    `json:"id"`
	ExtID    string `json:"ext_id"`
	Provider string `json:"provider"`
	Title    string `json:"title"`
	Year     int    `json:"year"`
	Poster   string `json:"poster"`
	Overview string `json:"overview"`
	IsTV     bool   `json:"is_tv,omitempty"`
}

// MatchID returns the provider-neutral match id: ExtID when set, else the
// legacy numeric TMDb id as a string, else "".
func (r Result) MatchID() string {
	if r.ExtID != "" {
		return r.ExtID
	}
	if r.ID != 0 {
		return strconv.Itoa(r.ID)
	}
	return ""
}

// episodeMarker matches episode indicators in disc titles.
var episodeMarker = regexp.MustCompile(`\s*[-–—]\s*(?:S\d+E\d+|Ep\s*\d+|Episode\s*\d+)\s*[-–—]?\s*.*`)

// ParseTitle extracts a clean title from a MakeMKV disc title string.
// It strips episode markers and trailing episode titles.
// Examples:
//
//	"Breaking Bad - S01E01 - Pilot" → "Breaking Bad"
//	"Inception" → "Inception"
//	"Avatar: The Way of Water" → "Avatar: The Way of Water"
//	"KLOVN - Ep 1" → "KLOVN"
func ParseTitle(raw string) string {
	if raw == "" {
		return ""
	}

	// Strip episode markers (e.g., " - S01E01 - ", " - Ep 1 - ")
	cleaned := episodeMarker.ReplaceAllString(raw, "")

	// Strip trailing episode title if present (split on " - ")
	parts := strings.SplitN(strings.TrimSpace(cleaned), " - ", 2)
	return strings.TrimSpace(parts[0])
}

var (
	seasonEpisodeRe  = regexp.MustCompile(`(?i)\bS(\d{1,2})\s*E(\d{1,3})\b`)
	seasonOnlyRe     = regexp.MustCompile(`(?i)\bSeason\s*(\d{1,2})\b|\bS(\d{1,2})\b`)
	episodeOnlyRe    = regexp.MustCompile(`(?i)\b(?:Ep|Episode)\s*(\d{1,3})\b`)
	trailingSeasonRe = regexp.MustCompile(`(?i)^(.*?)\s*[-–—:]?\s*(?:Season\s*\d{1,2}|S\d{1,2})$`)
)

// ParseSeasonEpisode extracts a season and/or episode marker from a disc title
// or filename. Missing values are 0. "KLOVN S01" → (1,0);
// "KLOVN - S02E03 - Pilot" → (2,3); "KLOVN - Ep 4" → (0,4).
func ParseSeasonEpisode(raw string) (season, episode int) {
	if raw == "" {
		return 0, 0
	}
	if m := seasonEpisodeRe.FindStringSubmatch(raw); m != nil {
		season, _ = strconv.Atoi(m[1])
		episode, _ = strconv.Atoi(m[2])
		return season, episode
	}
	if m := seasonOnlyRe.FindStringSubmatch(raw); m != nil {
		if m[1] != "" {
			season, _ = strconv.Atoi(m[1])
		} else if m[2] != "" {
			season, _ = strconv.Atoi(m[2])
		}
		return season, 0
	}
	if m := episodeOnlyRe.FindStringSubmatch(raw); m != nil {
		episode, _ = strconv.Atoi(m[1])
	}
	return 0, episode
}

// ParseSeriesTitle is ParseTitle with a trailing season marker removed:
// "KLOVN S01" → "KLOVN"; "Breaking Bad - S01E01 - Pilot" → "Breaking Bad".
func ParseSeriesTitle(raw string) string {
	t := strings.TrimSpace(ParseTitle(raw))
	if m := trailingSeasonRe.FindStringSubmatch(t); m != nil {
		t = strings.TrimSpace(m[1])
	}
	if t == "" {
		return strings.TrimSpace(ParseTitle(raw))
	}
	return t
}
