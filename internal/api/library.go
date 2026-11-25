package api

import (
	"context"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"riply/internal/drive"
	"riply/internal/library"
	"riply/internal/logger"
	"riply/internal/naming"
	"riply/internal/sse"
)

// ripMediaType maps a session's season number and classified media type to the
// library media_type: TV when either marks it as such, movie otherwise.
func ripMediaType(season int, mediaType string) string {
	if season > 0 || mediaType == "tv" {
		return "tv"
	}
	return "movie"
}

// autofillTV returns the season and starting episode for a TV rip. An explicit
// episode marker on the disc wins; otherwise the library's highest existing
// episode in the hinted (or highest) season determines the next one.
func (s *Server) autofillTV(discName string, tmdb *naming.Result, mediaType string) (season, episode int) {
	isTV := mediaType == "tv" || (tmdb != nil && tmdb.IsTV)
	if !isTV {
		return 0, 0
	}

	seasonHint, episodeHint := naming.ParseSeasonEpisode(discName)

	title, tmdbID := "", 0
	if tmdb != nil {
		title, tmdbID = tmdb.Title, tmdb.ID
	} else {
		title = naming.ParseSeriesTitle(discName)
	}

	libSeason, libEpisode := 0, 0
	if s.lib != nil && searchableName(title) {
		libSeason, libEpisode, _ = s.lib.NextUp(s.baseCtx, title, tmdbID, seasonHint)
	}

	season = seasonHint
	if season <= 0 {
		season = libSeason
	}
	if season <= 0 {
		season = 1
	}

	episode = episodeHint
	if episode <= 0 {
		episode = libEpisode
	}
	if episode <= 0 {
		episode = 1
	}

	logger.Info("Autofilled TV position", "disc", discName, "title", title, "season", season, "episode", episode)
	return season, episode
}

// recordRipEntry persists one moved file. Recording failures are logged and
// never fail the HTTP request.
func (s *Server) recordRipEntry(e library.Entry) {
	if s.lib == nil {
		logger.Debug("Library history disabled", "title", e.Title)
		return
	}
	e.Source = "rip"
	if e.CreatedAt == "" {
		e.CreatedAt = time.Now().UTC().Format(time.RFC3339)
	}
	// Store an absolute path: Scan records absolute paths and detects vanished
	// files by prefix against the absolute movies_dir, so a relative movies_dir
	// would otherwise leave rip entries that can never be flagged missing.
	if abs, err := filepath.Abs(e.OutputPath); err == nil {
		e.OutputPath = abs
	}
	if err := s.lib.Add(s.baseCtx, e); err != nil {
		logger.Warn("Failed to record library entry", "title", e.Title, "path", e.OutputPath, "error", err)
	}
}

// duplicateWarning checks library history for a previous rip of this disc and
// publishes a warning SSE event. Failures are logged, never fatal.
func (s *Server) duplicateWarning(ctx context.Context, drivePath, discName string) {
	if s.lib == nil || !searchableName(discName) {
		return
	}
	matches, err := s.lib.RipMatches(ctx, discName)
	if err != nil {
		logger.Warn("Duplicate check failed", "disc", discName, "error", err)
		return
	}
	if len(matches) == 0 {
		return
	}
	m := matches[0]
	label := m.Title
	if label == "" {
		label = m.DiscName
	}
	logger.Info("Disc was ripped before", "disc", discName, "title", m.Title, "season", m.Season, "path", m.OutputPath)
	s.sseMgr.Publish(sse.Event{
		Type:    "rip_duplicate",
		Drive:   drivePath,
		Message: fmt.Sprintf("Looks like you already ripped %q → %s", label, m.OutputPath),
	})
}

// suggestTV returns a season/episode suggestion for the search modal: an
// explicit episode marker wins; a season-only marker (or none) continues
// after the highest stored episode.
func (s *Server) suggestTV(discName string, tmdb *naming.Result) (season, episode int, ok bool) {
	seasonHint, episodeHint := naming.ParseSeasonEpisode(discName)
	if episodeHint > 0 {
		return seasonHint, episodeHint, true
	}
	title, tmdbID := "", 0
	if tmdb != nil {
		title, tmdbID = tmdb.Title, tmdb.ID
	} else {
		title = naming.ParseSeriesTitle(discName)
	}
	if s.lib == nil || !searchableName(title) {
		return 0, 0, false
	}
	season, episode, ok = s.lib.NextUp(s.baseCtx, title, tmdbID, seasonHint)
	return season, episode, ok
}

// nextUpInfo is the inline next-up suggestion on a drive card.
type nextUpInfo struct {
	Series  string `json:"series"`
	Season  int    `json:"season"`
	Episode int    `json:"episode"`
}

// lastRipInfo is the global last-rip suggestion for the search modal.
type lastRipInfo struct {
	Title     string `json:"title"`
	Year      int    `json:"year"`
	MediaType string `json:"media_type"`
	Season    int    `json:"season"`
	Episode   int    `json:"episode"`
	TMDBID    int    `json:"tmdb_id"`
}

type suggestInfo struct {
	Season  int `json:"season"`
	Episode int `json:"episode"`
}

// driveResponse decorates a drive with its library next-up position.
type driveResponse struct {
	drive.Drive
	NextUp    *nextUpInfo  `json:"next_up,omitempty"`
	LastRip   *lastRipInfo `json:"last_rip,omitempty"`
	Suggest   *suggestInfo `json:"suggest,omitempty"`
	MediaType string       `json:"media_type,omitempty"` // "movie" or "tv" from the TMDb match
}

// handleGetLibrary returns the full library snapshot.
func (s *Server) handleGetLibrary(w http.ResponseWriter, r *http.Request) {
	if s.lib == nil {
		http.Error(w, "library database unavailable", http.StatusServiceUnavailable)
		return
	}
	snap, err := s.lib.Snapshot(r.Context())
	if err != nil {
		logger.Error("Library snapshot failed", "error", err)
		http.Error(w, "library snapshot failed", http.StatusInternalServerError)
		return
	}
	writeJSON(w, snap)
}

// handleScanLibrary starts an asynchronous scan of movies_dir into the library.
func (s *Server) handleScanLibrary(w http.ResponseWriter, r *http.Request) {
	if s.lib == nil {
		http.Error(w, "library database unavailable", http.StatusServiceUnavailable)
		return
	}
	s.mu.Lock()
	moviesDir := s.cfg.MoviesDir
	s.mu.Unlock()
	if strings.TrimSpace(moviesDir) == "" {
		http.Error(w, "movies_dir is not configured", http.StatusBadRequest)
		return
	}
	if info, err := os.Stat(moviesDir); err != nil || !info.IsDir() {
		http.Error(w, "movies_dir is not a directory", http.StatusBadRequest)
		return
	}
	if !s.scanMu.TryLock() {
		http.Error(w, "scan already running", http.StatusConflict)
		return
	}

	w.WriteHeader(http.StatusAccepted)
	writeJSON(w, map[string]string{"status": "scanning"})

	go func() {
		defer s.scanMu.Unlock()
		res, err := s.lib.Scan(s.baseCtx, moviesDir)
		if err != nil {
			logger.Error("Library scan failed", "dir", moviesDir, "error", err)
			s.sseMgr.Publish(sse.Event{
				Type:    "library_scan_failed",
				Message: fmt.Sprintf("Library scan failed: %v", err),
			})
			return
		}
		s.sseMgr.Publish(sse.Event{
			Type: "library_scanned",
			Message: fmt.Sprintf("Scanned %d files: %d added, %d updated, %d missing",
				res.Added+res.Updated, res.Added, res.Updated, res.Missing),
		})
	}()
}

// handleResetLibrary clears the library index. It performs no filesystem
// calls: the database is an index of media, not the media itself.
func (s *Server) handleResetLibrary(w http.ResponseWriter, r *http.Request) {
	if s.lib == nil {
		http.Error(w, "library database unavailable", http.StatusServiceUnavailable)
		return
	}
	if !s.scanMu.TryLock() {
		http.Error(w, "scan already running", http.StatusConflict)
		return
	}
	defer s.scanMu.Unlock()

	if err := s.lib.Reset(r.Context()); err != nil {
		logger.Error("Library reset failed", "error", err)
		http.Error(w, "library reset failed", http.StatusInternalServerError)
		return
	}
	logger.Info("Library database reset", "path", s.cfgPath)
	s.sseMgr.Publish(sse.Event{Type: "library_reset", Message: "Library database reset"})
	writeJSON(w, map[string]string{"status": "reset"})
}
