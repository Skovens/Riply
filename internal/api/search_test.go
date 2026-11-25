package api

import (
	"net/http"
	"strings"
	"testing"

	"riply/internal/naming"
)

// H1: handleSearch must not echo upstream error text to the browser. The
// namer's error ("TMDb API key not set") must be replaced by the generic
// "search failed" body; the detail only goes to the server log.
func TestHandleSearchGenericErrorBody(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})
	s.namer = naming.NewClient("") // non-nil so the 503 guard passes; Search always errors

	rec := doJSON(t, s, http.MethodGet, "/api/search?q=Inception", nil)
	if rec.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want 500", rec.Code)
	}
	body := rec.Body.String()
	if strings.TrimSpace(body) != "search failed" {
		t.Errorf("body = %q, want %q", body, "search failed\n")
	}
	if strings.Contains(body, "TMDb") || strings.Contains(body, "key") {
		t.Errorf("body leaks upstream error detail: %q", body)
	}
}
