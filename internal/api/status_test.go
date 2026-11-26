package api

import (
	"context"
	"net/http"
	"strings"
	"testing"

	"riply/internal/rip"
)

// noRipper reports that MakeMKV is not installed; used for the missing
// /api/status path (instantRipper always reports available).
type noRipper struct{}

func (r *noRipper) GetDiscInfo(context.Context, string) (rip.DiscInfo, error) {
	return rip.DiscInfo{}, nil
}
func (r *noRipper) Rip(context.Context, string, string, int64) <-chan rip.RipProgress {
	return nil
}
func (r *noRipper) Eject(context.Context, string) error { return nil }
func (r *noRipper) Version() string                    { return "" }
func (r *noRipper) IsAvailable() bool                  { return false }

func TestStatusMakemkvAvailable(t *testing.T) {
	s, _ := newRipServer(t, &instantRipper{outName: "movie.mkv"})

	rec := doJSON(t, s, http.MethodGet, "/api/status", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", rec.Code)
	}
	body := rec.Body.String()
	// instantRipper.IsAvailable is true; version is "test".
	if !strings.Contains(body, `"makemkv_available":true`) {
		t.Errorf("body missing makemkv_available:true — %s", body)
	}
	if !strings.Contains(body, `"makemkv_version":"test"`) {
		t.Errorf("body missing makemkv_version — %s", body)
	}
	if !strings.Contains(body, `"version":"0.0.1-dev"`) {
		t.Errorf("body missing version — %s", body)
	}
}

func TestStatusMakemkvMissing(t *testing.T) {
	s, _ := newRipServer(t, &noRipper{})

	rec := doJSON(t, s, http.MethodGet, "/api/status", nil)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", rec.Code)
	}
	body := rec.Body.String()
	if !strings.Contains(body, `"makemkv_available":false`) {
		t.Errorf("body missing makemkv_available:false — %s", body)
	}
	// Version must be omitted when unavailable.
	if strings.Contains(body, "makemkv_version") {
		t.Errorf("body should not include makemkv_version when unavailable — %s", body)
	}
}

