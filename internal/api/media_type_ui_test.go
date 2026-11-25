package api

import (
	"encoding/json"
	"net/http"
	"strings"
	"testing"
	"time"

	"riply/internal/drive"
	"riply/internal/naming"
)

// TestDriveResponseExposesMediaType pins the API contract the UI badge relies
// on: a drive with a TMDb match carries media_type "tv"/"movie", and a drive
// without one omits it entirely (omitempty), so the frontend can distinguish
// "known type" from "no match yet".
func TestDriveResponseExposesMediaType(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	// Seed one movie and one TV match; the second drive stays unmatched.
	s.scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DriveDiscovered, Path: "/dev/sr1"})
	s.scanner.ApplyEvent(drive.DiscEvent{EventType: drive.DiscInserted, Path: "/dev/sr1"})
	s.mu.Lock()
	s.namingMatches["/dev/sr0"] = naming.Result{ID: 1, Title: "Inception", Year: 2010, IsTV: false}
	s.namingMatches["/dev/sr1"] = naming.Result{ID: 2, Title: "KLOVN", Year: 2010, IsTV: true}
	s.mu.Unlock()

	rec := doJSON(t, s, http.MethodGet, "/api/drives", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("drives = %d %s", rec.Code, rec.Body.String())
	}
	var out []map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatal(err)
	}
	if len(out) < 2 {
		t.Fatalf("drives = %d, want at least 2", len(out))
	}

	byPath := map[string]map[string]any{}
	for _, d := range out {
		byPath[d["path"].(string)] = d
	}

	if got := byPath["/dev/sr0"]["media_type"]; got != "movie" {
		t.Errorf("drive 0 media_type = %v, want movie", got)
	}
	if got := byPath["/dev/sr1"]["media_type"]; got != "tv" {
		t.Errorf("drive 1 media_type = %v, want tv", got)
	}
	for p, d := range byPath {
		if p == "/dev/sr0" || p == "/dev/sr1" {
			continue
		}
		if _, present := d["media_type"]; present {
			t.Errorf("drive %s unexpectedly carries media_type %v", p, d["media_type"])
		}
	}
}

// TestRipResponseCarriesResolvedMediaType pins the rip-card badge contract:
// after completion the session exposes the resolved media type. A movie TMDb
// match on a TV-classified disc must report "movie".
func TestRipResponseCarriesResolvedMediaType(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "title_t01.mkv"})
	// A movie match is stored as the auto-naming result before the rip starts.
	s.mu.Lock()
	s.namingMatches["/dev/sr0"] = naming.Result{ID: 7, Title: "The Mask", Year: 1994, IsTV: false}
	s.mu.Unlock()

	id := startRipViaAPI(t, s)
	if err := waitState(s, id, func(st string) bool { return st == "complete" || st == "error" }, 5*time.Second); err != nil {
		t.Fatalf("rip never completed: %v", err)
	}
	waitDone(s, id, time.Second)

	rec := doJSON(t, s, http.MethodGet, "/api/rips", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("rips = %d %s", rec.Code, rec.Body.String())
	}
	var rips []map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &rips); err != nil {
		t.Fatal(err)
	}
	var body string
	for _, r := range rips {
		if rid, _ := r["id"].(float64); int(rid) == id {
			b, _ := json.Marshal(r)
			body = string(b)
			break
		}
	}
	if body == "" {
		t.Fatalf("rip %d not in /api/rips: %s", id, rec.Body.String())
	}
	if !strings.Contains(body, `"media_type":"movie"`) {
		t.Errorf("rip response missing media_type movie — %s", body)
	}
}
