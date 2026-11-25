package api

import (
	"bytes"
	"net/http"
	"net/http/httptest"
	"testing"

	"riply/internal/naming"
)

// doRaw sends raw bytes to a path through the registered routes.
func doRaw(t *testing.T, s *Server, method, path string, body []byte) *httptest.ResponseRecorder {
	t.Helper()
	mux := http.NewServeMux()
	s.RegisterRoutes(mux)
	req := httptest.NewRequest(method, path, bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, req)
	return rec
}

// M7 table: oversized bodies rejected 400 (documented choice), malformed
// required bodies rejected 400, and the optional Accept bodies keep working
// with no body (frontend sends none) or with a valid one.
func TestBodyLimitsAndDecodeErrors(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})

	// Seed a complete session so accept-style endpoints reach body handling.
	s.mu.Lock()
	s.ripCount++
	id := s.ripCount
	sess := &RipSession{
		ID:        id,
		DrivePath: "/dev/sr0",
		DiscName:  "Inception",
		State:     "complete",
		OutputDir: t.TempDir(),
		Done:      make(chan struct{}),
	}
	close(sess.Done)
	s.rips[id] = sess
	s.mu.Unlock()

	huge := bytes.Repeat([]byte("a"), 4<<20) // 4 MiB > 2 MiB cap

	tests := []struct {
		name   string
		method string
		path   string
		body   []byte
		want   int
	}{
		// Oversized → 400 on every decoded endpoint (pick-one documented: 400).
		{"4MiB auto-naming", http.MethodPut, "/api/drives/0/auto-naming", huge, http.StatusBadRequest},
		{"4MiB config PUT", http.MethodPut, "/api/config", huge, http.StatusBadRequest},
		{"4MiB accept (optional body)", http.MethodPost, "/api/rips/1/accept", huge, http.StatusBadRequest},
		{"4MiB accept-title (required body)", http.MethodPost, "/api/rips/1/accept-title", huge, http.StatusBadRequest},
		// Malformed JSON → 400, including the previously-silent optional sites.
		{"malformed auto-naming", http.MethodPut, "/api/drives/0/auto-naming", []byte("{oops"), http.StatusBadRequest},
		{"malformed config PUT", http.MethodPut, "/api/config", []byte("{oops"), http.StatusBadRequest},
		{"malformed accept", http.MethodPost, "/api/rips/1/accept", []byte("{oops"), http.StatusBadRequest},
		{"malformed accept-title", http.MethodPost, "/api/rips/1/accept-title", []byte("{oops"), http.StatusBadRequest},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			rec := doRaw(t, s, tt.method, tt.path, tt.body)
			if rec.Code != tt.want {
				t.Errorf("%s %s: status = %d, want %d", tt.method, tt.path, rec.Code, tt.want)
			}
		})
	}

	// Empty body: optional endpoints must still work (app.js POSTs accept
	// without a body); required endpoints reject with 400.
	t.Run("empty optional accept works", func(t *testing.T) {
		s.mu.Lock()
		sess.TMDB = &naming.Result{ID: 1, Title: "Inception", Year: 2010}
		s.mu.Unlock()
		rec := doRaw(t, s, http.MethodPost, "/api/rips/"+itoa(id)+"/accept", nil)
		// Not 400: empty body is legal here. The move may fail deeper for
		// environmental reasons, but the body must not be the reason.
		if rec.Code == http.StatusBadRequest {
			t.Errorf("empty optional body rejected 400: %s", rec.Body.String())
		}
	})
	t.Run("empty required auto-naming 400", func(t *testing.T) {
		rec := doRaw(t, s, http.MethodPut, "/api/drives/0/auto-naming", nil)
		if rec.Code != http.StatusBadRequest {
			t.Errorf("empty required body: status = %d, want 400", rec.Code)
		}
	})
}
