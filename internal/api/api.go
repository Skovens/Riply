package api

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"riply/internal/config"
	"riply/internal/drive"
	"riply/internal/library"
	"riply/internal/logger"
	"riply/internal/metrics"
	"riply/internal/move"
	"riply/internal/naming"
	"riply/internal/rip"
	"riply/internal/sse"
	"riply/internal/version"
	"riply/web"
)

// searchableName reports whether name is usable as a TMDb query.
func searchableName(name string) bool {
	name = strings.TrimSpace(name)
	return name != "" && !strings.EqualFold(name, "unknown")
}

// maxRequestBody bounds every request body we decode — all are tiny JSON, so
// the limit only stops abusive or runaway clients (M7).
const maxRequestBody = 2 << 20 // 2 MiB

// decodeJSON decodes the request body into v under maxRequestBody.
// Empty body (io.EOF): required bodies get 400; optional ones (the two Accept
// endpoints — the frontend POSTs them with no body, see app.js acceptRip/
// acceptName) keep zero values and succeed. Malformed or oversized JSON
// answers 400 and returns false. 400 is the documented response to an
// oversized body: MaxBytesReader surfaces the failure mid-decode and these
// handlers treat any undecodable body the same way.
func decodeJSON(w http.ResponseWriter, r *http.Request, v any, required bool) bool {
	r.Body = http.MaxBytesReader(w, r.Body, maxRequestBody)
	if err := json.NewDecoder(r.Body).Decode(v); err != nil {
		if errors.Is(err, io.EOF) && !required {
			return true // no body provided; zero values are the contract
		}
		http.Error(w, "invalid request body", http.StatusBadRequest)
		return false
	}
	return true
}

// RipTitle represents a single title from a rip.
type RipTitle struct {
	SourceFile          string         `json:"source_file"`
	SizeMB              float64        `json:"size_mb"`
	IsMainFile          bool           `json:"is_main_file"`
	AcceptedName        string         `json:"accepted_name"`
	Year                int            `json:"year"`
	TMDB                *naming.Result `json:"tmdb,omitempty"`
	ProposedDestination string         `json:"proposed_destination"` // target path when accepted
	Destination         string         `json:"destination,omitempty"`
	TvSeason            int            `json:"tv_season,omitempty"`
	TvEpisode           int            `json:"tv_episode,omitempty"`
	State               string         `json:"state"`
	Error               string         `json:"error,omitempty"`
}

// PendingAccept stores a queued accept request to execute when rip completes.
type PendingAccept struct {
	TMDB   *naming.Result `json:"tmdb"`
	Source string         `json:"source"` // "auto" or "manual"
	// Season/Episode are explicit overrides from the accept request (N1).
	// 0 means "not specified" — the value from autofillTV is used.
	Season  int `json:"season,omitempty"`
	Episode int `json:"episode,omitempty"`
}

// RipState is the lifecycle state of a rip session. Mirrors drive.State:
// typed string constants so typos become compile errors, and the JSON wire
// format stays the same.
type RipState string

const (
	StateRipping   RipState = "ripping"
	StateQueued    RipState = "queued"
	StateComplete  RipState = "complete"
	StateAccepted  RipState = "accepted"
	StateError     RipState = "error"
	StateCancelled RipState = "cancelled"
)

// maxSeasonEpisode bounds accepted season/episode values (M8); negative or
// absurd values used to reach the move layer verbatim, producing Season -1/
// paths and S-1E-3 names.
const maxSeasonEpisode = 10000

// RipSession tracks an active or completed rip.
type RipSession struct {
	ID            int                `json:"id"`
	DrivePath     string             `json:"drive_path"`
	DiscName      string             `json:"disc_name"`
	State         string             `json:"state"` // "ripping", "queued", "complete", "accepted", "error"
	OutputDir     string             `json:"-"`     // internal: path to rip output directory
	OutputPath    string             `json:"output_path,omitempty"`
	TMDB          *naming.Result     `json:"tmdb,omitempty"`
	Progress      *sse.Progress      `json:"progress,omitempty"`
	DiscSizeBytes int64              `json:"-"` // media capacity in bytes; drives progress total estimate
	Error         string             `json:"error,omitempty"`
	MediaType     string             `json:"media_type,omitempty"`     // "movie", "tv", or "unknown"; effective type after TMDb override
	ParsedTitle   string             `json:"parsed_title,omitempty"`   // from ParseTitle()
	SeasonNumber  int                `json:"season_number,omitempty"`  // for TV shows
	EpisodeNumber int                `json:"episode_number,omitempty"` // for TV shows
	PendingAccept *PendingAccept     `json:"pending_accept,omitempty"` // queued accept data
	Cancel        context.CancelFunc `json:"-"`
	Done          chan struct{}      `json:"-"`
	// acceptMu serialises accepts for this session so two concurrent accepts
	// (drive-level auto-accept racing a manual one) cannot pick the same
	// destination folder or race over the rip output directory. Pointer so
	// snapshot copies (handleListRips) share the same lock and never copy it.
	acceptMu    *sync.Mutex
	CompletedAt time.Time   `json:"completed_at,omitempty"` // timestamp when rip reached terminal state
	Titles      []*RipTitle `json:"titles,omitempty"`       // per-title tracking for multi-title rips
}

// Server holds all dependencies and serves HTTP requests.
type Server struct {
	cfg     *config.Config
	cfgPath string // path to config file for persistence
	scanner *drive.Scanner
	ripper  rip.Ripper
	namer   *naming.Client
	sseMgr  *sse.Manager
	metrics *metrics.Metrics
	lib     *library.Store

	// scanMu serialises library scans; reset refuses to overlap a running scan.
	scanMu sync.Mutex

	// namingMatches maps drivePath → best TMDb match found by auto-naming.
	// Populated when a disc is inserted and TMDb search completes.
	namingMatches map[string]naming.Result
	// driveAccepts maps drivePath → name the user accepted before/without a rip.
	// Cleared on disc insert/eject and on manual rename; consumed when a rip starts.
	driveAccepts map[string]driveAccept
	// lastAutoEjectedDisc guards against auto-ripping the same disc that was
	// just auto-ejected. Keyed by drive path; value is the disc name.
	lastAutoEjectedDisc map[string]string
	mu                  sync.Mutex
	rips                map[int]*RipSession
	ripCount            int

	// Graceful shutdown fields.
	baseCtx      context.Context
	cancelBase   context.CancelFunc
	wg           sync.WaitGroup
	ripMu        sync.Mutex
	shuttingDown atomic.Bool

	// manualRipStarted is set true the first time the user manually starts a
	// rip. Until then, MaybeStartAutoRip is a no-op so Riply does not auto-rip
	// a disc that was already in the tray at startup.
	manualRipStarted atomic.Bool
}

// driveAccept is a pre-rip drive-level accept. Season/Episode (N1) carry an
// explicit TV position from the accept request; 0 means unset.
type driveAccept struct {
	result  naming.Result
	season  int
	episode int
}

// New creates a new Server.
func New(cfg *config.Config, cfgPath string, scanner *drive.Scanner, ripper rip.Ripper, namer *naming.Client, sseMgr *sse.Manager, m *metrics.Metrics, lib *library.Store) *Server {
	ctx, cancel := context.WithCancel(context.Background())
	return &Server{
		cfg:                 cfg,
		cfgPath:             cfgPath,
		scanner:             scanner,
		ripper:              ripper,
		namer:               namer,
		sseMgr:              sseMgr,
		metrics:             m,
		lib:                 lib,
		rips:                make(map[int]*RipSession),
		driveAccepts:        make(map[string]driveAccept),
		lastAutoEjectedDisc: make(map[string]string),
		namingMatches:       make(map[string]naming.Result),
		baseCtx:             ctx,
		cancelBase:          cancel,
	}
}

// acceptLock returns the session's accept mutex, allocating one if the
// session was created without it (tests seed sessions directly). Callers hold
// s.mu or are otherwise sole owners at that point.
func (sess *RipSession) acceptLock() *sync.Mutex {
	if sess.acceptMu == nil {
		sess.acceptMu = &sync.Mutex{}
	}
	return sess.acceptMu
}

// resolveDrive maps the {index} URL parameter to a drive path. When the
// client echoes ?path= (the frontend stores it per drive card), a mismatch
// means the drive list changed since the page rendered — e.g. a USB device
// was replugged and index N now addresses a different drive. Reject with 409
// instead of acting on the wrong device. On failure the error response is
// written here; ok=false means the caller must return immediately.
func (s *Server) resolveDrive(w http.ResponseWriter, r *http.Request) (string, bool) {
	index, err := strconv.Atoi(r.PathValue("index"))
	if err != nil {
		http.Error(w, "invalid drive index", http.StatusBadRequest)
		return "", false
	}
	drives := s.scanner.All()
	if index < 0 || index >= len(drives) {
		http.Error(w, "drive not found", http.StatusNotFound)
		return "", false
	}
	drivePath := drives[index].Path
	if want := r.URL.Query().Get("path"); want != "" && want != drivePath {
		logger.Warn("Drive changed since page render", "index", index, "expected", want, "found", drivePath)
		http.Error(w, "drive changed, refresh", http.StatusConflict)
		return "", false
	}
	return drivePath, true
}

// StartAutoNaming is a public wrapper for startAutoNaming.
func (s *Server) StartAutoNaming(drivePath, discName string) {
	s.startAutoNaming(drivePath, discName)
}

// DuplicateCheck publishes a warning if this disc was ripped before.
func (s *Server) DuplicateCheck(ctx context.Context, drivePath, discName string) {
	s.duplicateWarning(ctx, drivePath, discName)
}

// AutoSequenceDefault reports the global Auto-Sequence config default for a
// newly inserted disc.
func (s *Server) AutoSequenceDefault() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.cfg.AutoSequence
}

// AutoRipDefault reports the global Autorip config default for a newly
// inserted disc.
func (s *Server) AutoRipDefault() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.cfg.AutoRip
}

// MaybeStartAutoRip is called by the drive event loop when a new disc becomes
// Ready and Autorip is enabled for that drive. It guards against loops and
// races before starting the rip.
func (s *Server) MaybeStartAutoRip(drivePath, discName string) {
	if !s.manualRipStarted.Load() {
		logger.Debug("Autorip skipped: no manual rip started yet", "drive", drivePath)
		return
	}
	if s.ripper == nil || !s.ripper.IsAvailable() {
		logger.Debug("Autorip skipped: MakeMKV unavailable", "drive", drivePath)
		return
	}
	if s.shuttingDown.Load() {
		logger.Debug("Autorip skipped: shutting down", "drive", drivePath)
		return
	}

	d, ok := s.scanner.Snapshot(drivePath)
	if !ok || d.State != drive.StateReady || !d.AutoRipEnabled {
		logger.Debug("Autorip skipped: drive not ready or disabled", "drive", drivePath, "state", d.State, "enabled", d.AutoRipEnabled)
		return
	}

	s.mu.Lock()
	if last, ok := s.lastAutoEjectedDisc[drivePath]; ok && strings.EqualFold(last, discName) {
		delete(s.lastAutoEjectedDisc, drivePath)
		s.mu.Unlock()
		logger.Info("Autorip skipped: same disc just auto-ejected", "drive", drivePath, "disc", discName)
		return
	}
	for _, sess := range s.rips {
		if sess.DrivePath == drivePath && (sess.State == string(StateRipping) || sess.State == string(StateQueued)) {
			s.mu.Unlock()
			logger.Debug("Autorip skipped: rip already active", "drive", drivePath)
			return
		}
	}
	s.mu.Unlock()

	logger.Info("Auto-starting rip", "drive", drivePath, "disc", discName)
	s.sseMgr.Publish(sse.Event{
		Type:    "drive_loading",
		Drive:   drivePath,
		Message: "Autorip starting...",
	})
	if _, err := s.startRipForPath(drivePath); err != nil {
		logger.Warn("Autorip failed to start rip", "drive", drivePath, "error", err)
	}
}

// ClearNamingMatch removes any stored TMDb match for the drive.
func (s *Server) ClearNamingMatch(drivePath string) {
	s.mu.Lock()
	delete(s.namingMatches, drivePath)
	delete(s.driveAccepts, drivePath)
	s.mu.Unlock()
}

// RegisterRoutes sets up all HTTP routes on the stdlib http.ServeMux.
func (s *Server) RegisterRoutes(mux *http.ServeMux) {
	// API routes
	mux.HandleFunc("GET /api/status", s.handleStatus)
	mux.HandleFunc("GET /api/drives", s.handleDrives)
	mux.HandleFunc("POST /api/drives/{index}/start", s.handleStartRip)
	mux.HandleFunc("POST /api/drives/{index}/eject", s.handleEject)
	mux.HandleFunc("PUT /api/drives/{index}/auto-naming", s.handleToggleAutoNaming)
	mux.HandleFunc("PUT /api/drives/{index}/auto-sequence", s.handleToggleAutoSequence)
	mux.HandleFunc("PUT /api/drives/{index}/autorip", s.handleToggleAutoRip)
	mux.HandleFunc("PUT /api/drives/{index}/name", s.handleSetName)
	mux.HandleFunc("POST /api/drives/{index}/accept", s.handleDriveAccept)
	mux.HandleFunc("POST /api/drives/{index}/undo-accept", s.handleDriveUndoAccept)
	mux.HandleFunc("GET /api/rips", s.handleListRips)
	mux.HandleFunc("POST /api/rips/{id}/cancel", s.handleCancelRip)
	mux.HandleFunc("POST /api/rips/{id}/accept", s.handleAccept)
	mux.HandleFunc("POST /api/rips/{id}/accept-title", s.handleAcceptTitle)
	mux.HandleFunc("POST /api/rips/{id}/undo-accept", s.handleUndoAccept)
	mux.HandleFunc("GET /api/search", s.handleSearch)
	mux.HandleFunc("GET /api/library", s.handleGetLibrary)
	mux.HandleFunc("POST /api/library/scan", s.handleScanLibrary)
	mux.HandleFunc("POST /api/library/reset", s.handleResetLibrary)
	mux.HandleFunc("GET /api/config", s.handleGetConfig)
	mux.HandleFunc("PUT /api/config", s.handlePutConfig)
	mux.HandleFunc("GET /api/metrics", s.handleMetrics)

	// SSE endpoint
	mux.HandleFunc("GET /events", s.handleSSE)

	// Static files from the embedded asset tree.
	staticFS, err := fs.Sub(web.FS, "static")
	if err != nil {
		// Cannot happen for an embedded path; serve API routes only.
		logger.Error("Embedded static filesystem unavailable", "error", err)
		return
	}
	mux.Handle("GET /static/", http.StripPrefix("/static/", http.FileServerFS(staticFS)))

	// Frontend
	mux.HandleFunc("GET /", s.handleIndex)
}

// handleDrives lists all drives, merging auto-naming matches.
func (s *Server) handleDrives(w http.ResponseWriter, r *http.Request) {
	drives := s.scanner.All()

	// Merge naming matches into drive responses
	outMediaTypes := make([]string, len(drives))
	s.mu.Lock()
	for i := range drives {
		if match, ok := s.namingMatches[drives[i].Path]; ok {
			drives[i].CanonicalName = match.Title
			drives[i].CanonicalYear = match.Year
			drives[i].CanonicalID = match.ID
			drives[i].Poster = match.Poster
			// Snapshot the resolved type under lock; the out loop below runs
			// without s.mu and must not touch namingMatches again.
			outMediaTypes[i] = resolveMediaType("movie", &match)
		}
	}
	// Merge drive-level accepts (pre-accepted before rip started)
	for i := range drives {
		if acc, ok := s.driveAccepts[drives[i].Path]; ok {
			drives[i].AcceptedName = acc.result.Title
			drives[i].AcceptedYear = acc.result.Year
		}
	}
	s.mu.Unlock()

	s.mu.Lock()
	autoSeq := s.cfg.AutoSequence
	s.mu.Unlock()

	var lastRip *lastRipInfo
	if autoSeq && s.lib != nil {
		if lr, ok := s.lib.LastRip(r.Context()); ok {
			lastRip = &lastRipInfo{Title: lr.Title, Year: lr.Year, MediaType: lr.MediaType, Season: lr.Season, Episode: lr.Episode, TMDBID: lr.TMDBID}
		}
	}

	out := make([]driveResponse, len(drives))
	for i, d := range drives {
		resp := driveResponse{Drive: d}
		if outMediaTypes[i] != "" {
			resp.MediaType = outMediaTypes[i]
		}
		if s.lib != nil {
			series := d.CanonicalName
			if series == "" {
				series = naming.ParseSeriesTitle(d.DiscName)
			}
			if searchableName(series) {
				hint, _ := naming.ParseSeasonEpisode(d.DiscName)
				if season, episode, ok := s.lib.NextUp(r.Context(), series, d.CanonicalID, hint); ok {
					resp.NextUp = &nextUpInfo{Series: series, Season: season, Episode: episode}
				}
			}
		}
		if lastRip != nil && resp.AutoSequenceEnabled {
			resp.LastRip = lastRip
			if d.HasDisc {
				s.mu.Lock()
				match, haveMatch := s.namingMatches[d.Path]
				s.mu.Unlock()
				var matchPtr *naming.Result
				if haveMatch {
					m := match
					matchPtr = &m
				}
				if season, episode, ok := s.suggestTV(d.DiscName, matchPtr); ok {
					resp.Suggest = &suggestInfo{Season: season, Episode: episode}
				}
			}
		}
		out[i] = resp
	}

	writeJSON(w, out)
}

// startRipForPath starts a rip on a drive that is already verified Ready.
// It is used by both the manual HTTP handler and the Autorip hook.
func (s *Server) startRipForPath(drivePath string) (*RipSession, error) {
	// Reject non-ready drives before doing any work. This check must not sit
	// behind a makemkvcon probe: that probe retries for up to ~54s.
	d, ok := s.scanner.Snapshot(drivePath)
	if !ok || d.State != drive.StateReady {
		return nil, errors.New("no disc ready")
	}
	discSize := d.MediaSize
	discName := d.DiscName
	// Warn (without blocking) if library history shows a previous rip of this disc.
	s.duplicateWarning(s.baseCtx, drivePath, discName)

	// Register the rip with the shutdown WaitGroup only after every early
	// return is past; a count added before a 400/404/409 would leak and stall
	// Shutdown for its whole deadline. The lock prevents Add from racing with
	// Wait during shutdown.
	s.ripMu.Lock()
	if s.shuttingDown.Load() {
		s.ripMu.Unlock()
		return nil, errors.New("shutting down")
	}
	s.wg.Add(1)
	s.ripMu.Unlock()

	// Update drive state
	s.scanner.UpdateState(drivePath, drive.StateRipping, discName)

	// Build the output directory: canonical_name if set (manual naming),
	// otherwise the disc name (MakeMKV v2.0.0 refuses to overwrite existing
	// directories).
	timestamp := time.Now().Unix()
	dirName := discName
	s.mu.Lock()
	if match, ok := s.namingMatches[drivePath]; ok && match.Title != "" {
		dirName = match.Title
	}
	// Snapshot TempDir under lock to avoid race with concurrent PUT
	tempDir := s.cfg.TempDir
	s.mu.Unlock()
	outDir := filepath.Join(tempDir, fmt.Sprintf("%s_%d", move.SanitizeName(dirName), timestamp))

	// L7: best-effort preflight on the temp volume. A full temp dir fails the
	// rip midway (ripped titles, partial state); an early warning is better.
	// Unavailable stats are "unknown", not "full", so the rip still starts.
	if free, err := move.FreeSpace(tempDir); err != nil {
		logger.Debug("Free-space preflight unavailable", "temp_dir", tempDir, "error", err)
	} else {
		const gib = uint64(2 << 30)
		if free < gib {
			logger.Warn("Low free space on temp dir", "temp_dir", tempDir, "free_bytes", free, "free_mb", free/(1<<20))
		} else {
			logger.Info("Free-space preflight", "temp_dir", tempDir, "free_bytes", free, "free_mb", free/(1<<20))
		}
	}

	// Create cancel context before session struct. Drive addressing uses the
	// OS device path; the ripper maps it to its own enumeration (L4).
	ripCtx, ripCancel := context.WithCancel(s.baseCtx)
	done := make(chan struct{})

	// Create rip session
	s.mu.Lock()
	s.ripCount++
	session := &RipSession{
		ID:            s.ripCount,
		DrivePath:     drivePath,
		DiscName:      discName,
		State:         string(StateRipping),
		OutputDir:     outDir,
		Cancel:        ripCancel,
		Done:          done,
		DiscSizeBytes: discSize,
		acceptMu:      &sync.Mutex{},
	}
	if match, ok := s.namingMatches[drivePath]; ok {
		session.TMDB = &match
	}
	// Consume pre-accept if present (wins over auto-match). N1: carry the
	// explicit TV position through PendingAccept so the move lands Season N/.
	if acc, ok := s.driveAccepts[drivePath]; ok {
		a := acc.result
		session.TMDB = &a
		session.PendingAccept = &PendingAccept{TMDB: &a, Source: a.Title, Season: acc.season, Episode: acc.episode}
		session.State = string(StateQueued)
		delete(s.driveAccepts, drivePath)
	}
	s.rips[s.ripCount] = session
	s.mu.Unlock()

	s.metrics.ActiveRips.Add(1)
	go func() {
		defer s.wg.Done()
		defer close(done)
		logger.Info("Rip", "drive_path", drivePath, "operation", "rip")
		progressCh := s.ripper.Rip(ripCtx, drivePath, outDir, session.DiscSizeBytes)
		for p := range progressCh {
			s.mu.Lock()
			switch p.Status {
			case "ripping":
				session.Progress = &sse.Progress{Title: p.Title, Percent: p.Percent, SpeedMBPS: p.SpeedMBPS, TotalBytes: p.TotalBytes, EstimatedTotal: p.EstimatedTotal, Phase: p.Phase, TitleIndex: p.TitleIndex, TitleTotal: p.TitleTotal, RipPhase: p.RipPhase}
			case "error":
				if session.State != string(StateCancelled) {
					session.State = string(StateError)
					session.Error = p.Message
					session.CompletedAt = time.Now()
				}
			}
			s.mu.Unlock()

			if p.Status == "ripping" {
				logger.Debug("Rip progress", "phase", p.Phase, "percent", fmt.Sprintf("%.1f", p.Percent), "title_index", p.TitleIndex, "title_total", p.TitleTotal)
				var msg string
				if p.RipPhase == "scanning" {
					msg = fmt.Sprintf("Scanning disc: %s", p.Phase)
				} else {
					msg = fmt.Sprintf("Ripping: %.0f%%", p.Percent)
					if p.Phase != "" {
						msg = fmt.Sprintf("%s: %.0f%%", p.Phase, p.Percent)
					}
					if p.TitleTotal > 0 {
						msg = fmt.Sprintf("%s · saving title %d of %d", msg, p.TitleIndex, p.TitleTotal)
					}
				}
				s.sseMgr.Publish(sse.Event{
					Type:     "rip_progress",
					Drive:    drivePath,
					Message:  msg,
					Progress: &sse.Progress{Title: p.Title, Percent: p.Percent, SpeedMBPS: p.SpeedMBPS, TotalBytes: p.TotalBytes, EstimatedTotal: p.EstimatedTotal, Phase: p.Phase, TitleIndex: p.TitleIndex, TitleTotal: p.TitleTotal, RipPhase: p.RipPhase},
				})
			}
		}

		s.metrics.ActiveRips.Add(-1)
		s.metrics.TotalRips.Add(1)

		s.mu.Lock()
		failed := session.State == string(StateError)
		cancelled := session.State == string(StateCancelled) || ripCtx.Err() != nil
		errMsg := session.Error
		queuedAccept := !failed && !cancelled && session.State == string(StateQueued) && session.PendingAccept != nil
		if !failed && !cancelled {
			session.State = string(StateComplete)
			session.Progress = &sse.Progress{Percent: 100}
			session.CompletedAt = time.Now()
		}
		s.mu.Unlock()

		if failed {
			logger.Warn("Rip failed", "drive", drivePath, "error", errMsg)
			s.metrics.FailedRips.Add(1)
			s.scanner.UpdateState(drivePath, drive.StateError, session.DiscName)
			s.sseMgr.Publish(sse.Event{Type: "rip_failed", Drive: drivePath, Message: fmt.Sprintf("Rip failed: %s", errMsg)})
			return
		}

		if cancelled {
			logger.Info("Rip cancelled", "drive", drivePath)
			s.scanner.UpdateState(drivePath, drive.StateReady, session.DiscName)
			s.sseMgr.Publish(sse.Event{Type: "rip_cancelled", Drive: drivePath, Message: "Rip cancelled"})
			return
		}

		logger.Info("Rip complete", "drive", drivePath)

		// Smart classification
		mediaType, medianMB := classifyRip(outDir)
		logger.Debug("classifyRip", "type", mediaType, "median_mb", fmt.Sprintf("%.1f", medianMB))

		// Parse title for display
		parsedTitle := naming.ParseTitle(discName)

		// Use pre-computed auto-naming match if available
		s.mu.Lock()
		session.MediaType = mediaType
		session.ParsedTitle = parsedTitle
		if match, ok := s.namingMatches[drivePath]; ok {
			session.TMDB = &match
			logger.Info("Auto-naming match available", "title", match.Title, "year", match.Year)
		} else if s.namer != nil && s.namer.APIKey() != "" {
			// Fallback: search after rip if no auto-match was found
			cleanName := parsedTitle
			if cleanName == "" {
				cleanName = discName
			}
			if !searchableName(cleanName) {
				logger.Info("Skipping post-rip TMDb search", "drive", drivePath, "parsed", parsedTitle, "disc", discName)
			} else {
				results, err := s.namer.Search(s.baseCtx, cleanName)
				if err == nil && len(results) > 0 {
					session.TMDB = &results[0]
					logger.Info("Naming suggestion", "title", results[0].Title, "year", results[0].Year, "disc", discName)
				} else if err != nil {
					logger.Error("TMDb search after rip failed", "error", err)
				}
			}
		}
		// Snapshot MoviesDir and session fields under lock to avoid races with concurrent PUT/accept
		moviesDir := s.cfg.MoviesDir
		sessionTMDB := session.TMDB
		sessionMediaType := session.MediaType
		sessionDiscName := session.DiscName
		s.mu.Unlock()

		// The TMDb match is authoritative over the file-size heuristic: a movie
		// match forces the movie layout (extras to Extras/), a TV match forces
		// the TV layout, so a misclassified disc still lands correctly.
		effectiveType := resolveMediaType(sessionMediaType, sessionTMDB)
		logger.Info("Resolved media type", "classified", sessionMediaType, "tmdb_is_tv", sessionTMDB != nil && sessionTMDB.IsTV, "effective", effectiveType)

		// Autofill the TV position from library history: an explicit marker on
		// the disc wins, otherwise continue after the highest stored episode.
		season, episode := s.autofillTV(sessionDiscName, sessionTMDB, effectiveType)

		// Populate per-title list
		titles := populateTitles(outDir, moviesDir, sessionTMDB, effectiveType, season, episode)
		s.mu.Lock()
		session.Titles = titles
		session.SeasonNumber = season
		session.EpisodeNumber = episode
		session.MediaType = effectiveType
		s.mu.Unlock()

		// Auto-execute queued accept if one was pending
		s.mu.Lock()
		var records []library.Entry
		if queuedAccept && session.PendingAccept != nil && session.TMDB != nil {
			pa := session.PendingAccept
			session.PendingAccept = nil
			session.TMDB = pa.TMDB
			title := pa.TMDB.Title
			year := pa.TMDB.Year
			ripDir := session.OutputDir
			// Snapshot MoviesDir under lock to avoid race with concurrent PUT
			moviesDir := s.cfg.MoviesDir
			tvSeason, tvEpisode := session.SeasonNumber, session.EpisodeNumber
			// N1: an explicit season/episode on the accept request (a
			// drive-level accept that knew the TV position) overrides the
			// autofilled value. Without one, a TV naming match still forces
			// the TV layout — a single-episode disc fools classifyRip's
			// size heuristic into "movie".
			if pa.TMDB.IsTV || pa.Season > 0 {
				if tvSeason <= 0 {
					tvSeason = 1
				}
				if pa.Season > 0 {
					tvSeason = pa.Season
				}
				if tvEpisode <= 0 {
					tvEpisode = 1
				}
				if pa.Episode > 0 {
					tvEpisode = pa.Episode
				}
				session.SeasonNumber = tvSeason
				session.EpisodeNumber = tvEpisode
			}
			logger.Info("Auto-executing queued accept", "id", session.ID, "title", title, "year", year,
				"season", tvSeason, "episode", tvEpisode, "media_type", session.MediaType)

			s.mu.Unlock()

			session.acceptLock().Lock()
			result, err := move.Move(ripDir, moviesDir, title, strconv.Itoa(year), tvSeason, tvEpisode)
			if err != nil {
				failed := 0
				if result != nil {
					failed = result.FailedExtras
				}
				logger.Error("Auto-execute move failed", "title", title, "failed_extras", failed, "error", err)
			} else {
				s.mu.Lock()
				session.State = string(StateAccepted)
				session.OutputPath = result.Destination
				session.CompletedAt = time.Now()
				sessDisc := session.DiscName
				mt := session.MediaType
				s.mu.Unlock()
				movedAt := time.Now().UTC().Format(time.RFC3339)
				if len(result.Episodes) > 0 {
					// N1: a TV move lands one file per episode; record each
					// so library next-up advances over all of them.
					for _, me := range result.Episodes {
						records = append(records, library.Entry{
							OutputPath: me.Path,
							DiscName:   sessDisc,
							Title:      title,
							Year:       year,
							MediaType:  "tv",
							Season:     tvSeason,
							Episode:    me.Episode,
							TMDBID:     pa.TMDB.ID,
							MovedAt:    movedAt,
						})
					}
				} else {
					records = append(records, library.Entry{
						OutputPath: result.MainFile,
						DiscName:   sessDisc,
						Title:      title,
						Year:       year,
						MediaType:  ripMediaType(tvSeason, resolveMediaType(mt, pa.TMDB)),
						Season:     tvSeason,
						Episode:    tvEpisode,
						TMDBID:     pa.TMDB.ID,
						MovedAt:    movedAt,
					})
				}

				logger.Info("Auto-moved to", "destination", result.Destination)

				s.sseMgr.Publish(sse.Event{
					Type:    "rip_accepted",
					Drive:   drivePath,
					Message: fmt.Sprintf("Auto-moved to %s", result.Destination),
				})
				s.maybeAutoEject(drivePath, discName)
			}
			session.acceptLock().Unlock()
			s.mu.Lock()
		} else if queuedAccept {
			// The queued accept could not run; drop it so the session does
			// not advertise a queued accept it will never execute.
			session.PendingAccept = nil
		}
		s.mu.Unlock()
		for _, rec := range records {
			s.recordRipEntry(rec)
		}

		// Update drive state
		s.scanner.UpdateState(drivePath, drive.StateComplete, discName)
		s.metrics.SuccessfulRips.Add(1)

		s.sseMgr.Publish(sse.Event{
			Type:    "rip_complete",
			Drive:   drivePath,
			Message: "Rip completed",
		})
	}()

	return session, nil
}

// handleStartRip starts a rip on the given drive.
func (s *Server) handleStartRip(w http.ResponseWriter, r *http.Request) {
	if s.ripper == nil || !s.ripper.IsAvailable() {
		http.Error(w, "MakeMKV not installed", http.StatusServiceUnavailable)
		return
	}

	drivePath, ok := s.resolveDrive(w, r)
	if !ok {
		return
	}

	session, err := s.startRipForPath(drivePath)
	if err != nil {
		http.Error(w, err.Error(), http.StatusConflict)
		return
	}

	// First manual rip of the session enables autorip for later inserts.
	s.manualRipStarted.Store(true)
	logger.Info("Manual rip started; autorip enabled for session", "drive", drivePath)

	writeJSON(w, map[string]int{"rip_id": session.ID})
}

// startAutoNaming kicks off a TMDb search when a disc is inserted.
// It checks the auto-naming toggle for the drive before proceeding.
func (s *Server) startAutoNaming(drivePath, discName string) {
	if s.namer == nil || s.namer.APIKey() == "" {
		return
	}

	drive, ok := s.scanner.Snapshot(drivePath)
	if !ok || !drive.AutoNamingEnabled {
		logger.Debug("Auto-naming disabled", "drive", drivePath)
		return
	}

	// Clean the disc name for better search results
	cleanName := naming.ParseTitle(discName)
	if !searchableName(cleanName) {
		logger.Debug("Skipping auto-naming", "drive", drivePath, "reason", "empty/unknown disc name")
		return
	}

	logger.Info("Auto-naming started", "drive", drivePath, "query", cleanName)

	s.sseMgr.Publish(sse.Event{
		Type:    "naming_started",
		Drive:   drivePath,
		Message: fmt.Sprintf("Searching TMDb for %q...", cleanName),
	})

	go func() {
		results, err := s.namer.Search(s.baseCtx, cleanName)
		if err != nil {
			s.sseMgr.Publish(sse.Event{
				Type:    "naming_failed",
				Drive:   drivePath,
				Message: fmt.Sprintf("Auto-naming failed: %v", err),
			})
			return
		}
		if len(results) == 0 {
			s.sseMgr.Publish(sse.Event{
				Type:    "naming_failed",
				Drive:   drivePath,
				Message: "Auto-naming: no TMDb match found",
			})
			return
		}

		// Store the top match on the drive for later use
		s.mu.Lock()
		s.namingMatches[drivePath] = results[0]
		for _, sess := range s.rips {
			if sess.DrivePath == drivePath && sess.TMDB == nil &&
				(sess.State == string(StateRipping) || sess.State == string(StateQueued)) {
				m := results[0]
				sess.TMDB = &m
			}
		}
		s.mu.Unlock()

		logger.Info("Auto-named", "title", results[0].Title, "year", results[0].Year, "disc", discName)

		s.sseMgr.Publish(sse.Event{
			Type:    "naming_match",
			Drive:   drivePath,
			Message: fmt.Sprintf("Found TMDb match: %s (%d)", results[0].Title, results[0].Year),
		})
	}()
}

// classifyRip determines if a rip is a movie or TV show based on file sizes.
// Returns (type, medianSizeMB).
func classifyRip(outDir string) (string, float64) {
	entries, err := os.ReadDir(outDir)
	if err != nil {
		return "unknown", 0
	}

	var sizes []float64
	for _, e := range entries {
		if !e.IsDir() && strings.HasSuffix(strings.ToLower(e.Name()), ".mkv") {
			info, err := e.Info()
			if err == nil {
				sizes = append(sizes, float64(info.Size())/1024/1024)
			}
		}
	}

	if len(sizes) == 0 {
		return "unknown", 0
	}

	// Sort and find median
	sort.Float64s(sizes)
	median := sizes[len(sizes)/2]

	// If all files are similar size → movie (single title)
	// If files vary significantly → TV (multiple episodes)
	if len(sizes) <= 2 {
		return "movie", median
	}

	// Multiple files: check if sizes are similar (movie with extras) or varied (TV episodes)
	max, min := sizes[len(sizes)-1], sizes[0]
	ratio := float64(min) / float64(max)

	if ratio > 0.5 {
		// Files are similar size — likely extras, not TV
		return "movie", median
	}
	// Files vary significantly — likely TV episodes
	return "tv", median
}

// resolveMediaType determines whether a rip is a movie or TV show. The TMDb
// match is authoritative when present: a TV match forces the "tv" layout, a
// movie match forces the "movie" layout (extras go to Extras/). Without a
// match, the file-size heuristic from classifyRip is used as-is.
func resolveMediaType(classified string, tmdb *naming.Result) string {
	if tmdb == nil {
		return classified
	}
	if tmdb.IsTV {
		return "tv"
	}
	return "movie"
}

// handleToggleAutoNaming toggles auto-naming for a drive.
func (s *Server) handleToggleAutoNaming(w http.ResponseWriter, r *http.Request) {
	drivePath, ok := s.resolveDrive(w, r)
	if !ok {
		return
	}

	var body struct {
		Enabled bool `json:"enabled"`
	}
	if !decodeJSON(w, r, &body, true) {
		return
	}

	s.scanner.SetAutoNaming(drivePath, body.Enabled)
	logger.Info("Auto-naming set", "enabled", body.Enabled, "drive", drivePath)

	writeJSON(w, map[string]string{"status": "ok", "auto_naming": fmt.Sprintf("%v", body.Enabled)})
}

// handleToggleAutoSequence toggles Auto-Sequence for a drive.
func (s *Server) handleToggleAutoSequence(w http.ResponseWriter, r *http.Request) {
	drivePath, ok := s.resolveDrive(w, r)
	if !ok {
		return
	}

	var body struct {
		Enabled bool `json:"enabled"`
	}
	if !decodeJSON(w, r, &body, true) {
		return
	}

	s.scanner.SetAutoSequence(drivePath, body.Enabled)
	logger.Info("Auto-sequence set", "enabled", body.Enabled, "drive", drivePath)

	writeJSON(w, map[string]string{"status": "ok", "auto_sequence": fmt.Sprintf("%v", body.Enabled)})
}

// handleToggleAutoRip toggles Autorip for a drive.
func (s *Server) handleToggleAutoRip(w http.ResponseWriter, r *http.Request) {
	drivePath, ok := s.resolveDrive(w, r)
	if !ok {
		return
	}

	var body struct {
		Enabled bool `json:"enabled"`
	}
	if !decodeJSON(w, r, &body, true) {
		return
	}

	s.scanner.SetAutoRip(drivePath, body.Enabled)
	logger.Info("Autorip set", "enabled", body.Enabled, "drive", drivePath)

	writeJSON(w, map[string]string{"status": "ok", "autorip": fmt.Sprintf("%v", body.Enabled)})
}

// handleSetName sets a canonical name for a drive, overwriting any auto-named match.
func (s *Server) handleSetName(w http.ResponseWriter, r *http.Request) {
	drivePath, ok := s.resolveDrive(w, r)
	if !ok {
		return
	}

	var body struct {
		CanonicalName string `json:"canonical_name"`
		TMDBID        int    `json:"tmdb_id"`
		Title         string `json:"title"`
		Year          int    `json:"year"`
		Poster        string `json:"poster"`
		IsTV          bool   `json:"is_tv"`
	}
	if !decodeJSON(w, r, &body, true) {
		return
	}

	name := body.CanonicalName
	if name == "" {
		name = body.Title
	}
	if name == "" {
		http.Error(w, "canonical_name or title is required", http.StatusBadRequest)
		return
	}

	s.mu.Lock()
	s.namingMatches[drivePath] = naming.Result{
		ID:     body.TMDBID,
		Title:  name,
		Year:   body.Year,
		Poster: body.Poster,
		IsTV:   body.IsTV,
	}
	delete(s.driveAccepts, drivePath)
	s.mu.Unlock()

	s.sseMgr.Publish(sse.Event{
		Type:    "naming_match",
		Drive:   drivePath,
		Message: fmt.Sprintf("Name set: %s", name),
	})
	logger.Info("Manual name set", "drive", drivePath, "name", name)

	writeJSON(w, map[string]string{"status": "ok", "canonical_name": name})
}

// handleDriveAccept accepts a naming suggestion for a drive before or without a rip.
func (s *Server) handleDriveAccept(w http.ResponseWriter, r *http.Request) {
	drivePath, ok := s.resolveDrive(w, r)
	if !ok {
		return
	}

	// Parse optional request body
	var body struct {
		TMDBID    int    `json:"tmdb_id"`
		Title     string `json:"title"`
		Year      int    `json:"year"`
		Poster    string `json:"poster"`
		IsTV      bool   `json:"is_tv"`
		TvSeason  int    `json:"tv_season"`
		TvEpisode int    `json:"tv_episode"`
	}
	if !decodeJSON(w, r, &body, false) {
		return
	}

	// N1: an accept may carry the disc's TV position; validate the same
	// bounds accept-title uses so garbage never reaches Move (autofill can
	// still classify the rip as TV later, at which point a stored negative
	// season would produce Season -1/ paths).
	if body.TvSeason < 0 || body.TvSeason > maxSeasonEpisode ||
		body.TvEpisode < 0 || body.TvEpisode > maxSeasonEpisode {
		http.Error(w, "tv_season and tv_episode must be between 0 and 10000", http.StatusBadRequest)
		return
	}

	s.mu.Lock()

	// 409 if a rip is already running for this drive
	for _, sess := range s.rips {
		if sess.DrivePath == drivePath && (sess.State == string(StateRipping) || sess.State == string(StateQueued)) {
			s.mu.Unlock()
			http.Error(w, "rip in progress, use /api/rips/{id}/accept", http.StatusConflict)
			return
		}
	}

	// Resolve name: body.Title > namingMatches. N1: on fallback inherit the
	// stored match's TV flag so a bodyless Accept of a TMDb-matched TV disc
	// still lands the Season N/ layout even if classifyRip guesses "movie"
	// (single-episode discs fool the size heuristic). Adopt the whole stored
	// match (year, id, poster) too: the front-end's default Accept posts an
	// empty body, and dropping Year made the move land in "The Mask/" instead
	// of "The Mask (1994)/" (issue #98).
	name := body.Title
	if name == "" {
		if match, ok := s.namingMatches[drivePath]; ok && match.Title != "" {
			body.TMDBID = match.ID
			body.Title = match.Title
			body.Year = match.Year
			body.Poster = match.Poster
			body.IsTV = match.IsTV
			name = match.Title
		}
	}
	if name == "" {
		s.mu.Unlock()
		http.Error(w, "no naming suggestion available", http.StatusBadRequest)
		return
	}

	s.driveAccepts[drivePath] = driveAccept{
		result: naming.Result{
			ID:     body.TMDBID,
			Title:  name,
			Year:   body.Year,
			Poster: body.Poster,
			IsTV:   body.IsTV,
		},
		season:  body.TvSeason,
		episode: body.TvEpisode,
	}
	s.mu.Unlock()

	s.sseMgr.Publish(sse.Event{
		Type:    "drive_accepted",
		Drive:   drivePath,
		Message: fmt.Sprintf("Name accepted: %s", name),
	})
	logger.Info("Drive accept", "drive", drivePath, "title", name)

	writeJSON(w, map[string]string{"status": "accepted", "title": name})
}

// handleDriveUndoAccept cancels a pre-accepted name for a drive.
func (s *Server) handleDriveUndoAccept(w http.ResponseWriter, r *http.Request) {
	drivePath, ok := s.resolveDrive(w, r)
	if !ok {
		return
	}

	s.mu.Lock()
	delete(s.driveAccepts, drivePath)
	s.mu.Unlock()

	logger.Info("Drive undo-accept", "drive", drivePath)
	writeJSON(w, map[string]string{"status": "ok", "message": "Accept cancelled"})
}

// handleCancelRip cancels an active rip by ID.
func (s *Server) handleCancelRip(w http.ResponseWriter, r *http.Request) {
	id, err := strconv.Atoi(r.PathValue("id"))
	if err != nil {
		http.Error(w, "invalid rip id", http.StatusBadRequest)
		return
	}

	s.mu.Lock()
	session, ok := s.rips[id]
	if !ok {
		s.mu.Unlock()
		http.Error(w, "rip not found", http.StatusNotFound)
		return
	}
	if session.State != string(StateRipping) && session.State != string(StateQueued) {
		s.mu.Unlock()
		http.Error(w, "rip is not running", http.StatusConflict)
		return
	}
	session.State = string(StateCancelled)
	session.PendingAccept = nil
	session.CompletedAt = time.Now()
	cancel := session.Cancel
	s.mu.Unlock()

	if cancel != nil {
		cancel()
	}
	logger.Info("Rip cancel requested", "id", id)
	writeJSON(w, map[string]string{"status": "cancelled"})
}

// ejectDrive performs the physical eject and updates scanner state. It is used
// by both the manual HTTP handler and the Autorip auto-eject path.
func (s *Server) ejectDrive(ctx context.Context, drivePath string) error {
	logger.Info("Eject", "drive_path", drivePath, "operation", "eject")
	if err := s.ripper.Eject(ctx, drivePath); err != nil {
		logger.Error("Eject failed", "drive_path", drivePath, "error", err)
		return err
	}

	s.scanner.UpdateState(drivePath, drive.StateNoDisc, "")
	s.sseMgr.Publish(sse.Event{
		Type:    "drive_ejected",
		Drive:   drivePath,
		Message: "Disc ejected",
	})

	return nil
}

// maybeAutoEject ejects the drive after a successful accept when Autorip is
// enabled. It records the disc name so the next insert of the same disc does
// not immediately auto-rip again.
func (s *Server) maybeAutoEject(drivePath, discName string) {
	if drivePath == "" || discName == "" {
		return
	}
	d, ok := s.scanner.Snapshot(drivePath)
	if !ok || !d.AutoRipEnabled {
		logger.Debug("Auto-eject skipped: autorip disabled", "drive", drivePath)
		return
	}

	s.mu.Lock()
	s.lastAutoEjectedDisc[drivePath] = discName
	s.mu.Unlock()

	logger.Info("Auto-ejecting drive after accept", "drive", drivePath, "disc", discName)
	if err := s.ejectDrive(s.baseCtx, drivePath); err != nil {
		logger.Error("Auto-eject failed", "drive", drivePath, "error", err)
	}
}

// handleEject ejects the disc from the given drive.
func (s *Server) handleEject(w http.ResponseWriter, r *http.Request) {
	if s.ripper == nil || !s.ripper.IsAvailable() {
		http.Error(w, "MakeMKV not installed", http.StatusServiceUnavailable)
		return
	}

	drivePath, ok := s.resolveDrive(w, r)
	if !ok {
		return
	}

	// Cancel any active rip for this drive before ejecting
	s.mu.Lock()
	var done chan struct{}
	var cancel context.CancelFunc
	for _, sess := range s.rips {
		if sess.DrivePath == drivePath && (sess.State == string(StateRipping) || sess.State == string(StateQueued)) {
			sess.State = string(StateCancelled)
			sess.PendingAccept = nil
			sess.CompletedAt = time.Now()
			done, cancel = sess.Done, sess.Cancel
			break
		}
	}
	delete(s.driveAccepts, drivePath)
	s.mu.Unlock()

	if cancel != nil {
		logger.Info("Eject cancelling active rip", "drive_path", drivePath)
		cancel()
		if done != nil {
			select {
			case <-done:
			case <-time.After(15 * time.Second):
				logger.Warn("Timed out waiting for rip to stop before eject", "drive_path", drivePath)
			}
		}
	}

	if err := s.ejectDrive(r.Context(), drivePath); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, map[string]string{"status": "ok"})
}

// handleListRips lists all rips (including in-progress), newest first.
func (s *Server) handleListRips(w http.ResponseWriter, r *http.Request) {
	s.mu.Lock()
	result := make([]RipSession, 0, len(s.rips))
	for _, rip := range s.rips {
		if rip.State == string(StateAccepted) {
			continue
		}
		snap := *rip
		if rip.Progress != nil {
			p := *rip.Progress
			snap.Progress = &p
		}
		if rip.PendingAccept != nil {
			pa := *rip.PendingAccept
			snap.PendingAccept = &pa
		}
		if rip.Titles != nil {
			titles := make([]*RipTitle, len(rip.Titles))
			for i, t := range rip.Titles {
				tc := *t
				titles[i] = &tc
			}
			snap.Titles = titles
		}
		result = append(result, snap)
	}
	s.mu.Unlock()

	sort.Slice(result, func(i, j int) bool { return result[i].ID > result[j].ID })
	writeJSON(w, result)
}

// Shutdown gracefully shuts down the server: stops accepting new rips,
// cancels the base context (killing in-flight makemkvcon), publishes
// an SSE shutdown event, closes all SSE connections, and waits for
// active rips to finish.
func (s *Server) Shutdown(shutdownCtx context.Context) error {
	// 1. Lock ripMu, set shuttingDown — no new Add can occur.
	s.ripMu.Lock()
	s.shuttingDown.Store(true)
	s.ripMu.Unlock()

	// 2. Cancel base context — kills makemkvcon via exec.CommandContext.
	s.cancelBase()

	// 3. Publish SSE shutdown event.
	s.sseMgr.Publish(sse.Event{Type: "shutdown", Message: "Server shutting down"})

	// 4. Close all SSE connections.
	s.sseMgr.CloseAll()

	// 5. Wait for active rips with deadline.
	done := make(chan struct{})
	go func() {
		s.wg.Wait()
		close(done)
	}()
	select {
	case <-done:
	case <-shutdownCtx.Done():
		return shutdownCtx.Err()
	}

	// 6. Snapshot sessions under lock, then do filesystem work outside.
	s.mu.Lock()
	sessions := make([]*RipSession, 0, len(s.rips))
	for _, session := range s.rips {
		sessions = append(sessions, session)
	}
	s.mu.Unlock()

	for _, session := range sessions {
		if session.State == string(StateComplete) || session.State == string(StateAccepted) {
			continue
		}
		matches, err := filepath.Glob(filepath.Join(session.OutputDir, "*.mkv"))
		if err != nil {
			logger.Warn("Glob failed for partial rip dir", "path", session.OutputDir, "error", err)
			continue
		}
		if len(matches) == 0 {
			if err := os.RemoveAll(session.OutputDir); err != nil {
				logger.Warn("Failed to remove empty rip dir", "path", session.OutputDir, "error", err)
			} else {
				logger.Info("removed empty rip dir", "path", session.OutputDir)
			}
		} else {
			logger.Info("partial rip kept", "path", session.OutputDir)
		}
	}

	return nil
}

// handleAccept accepts a naming suggestion and moves to library.
// If the rip is still in progress, the accept is queued and will execute
// when the rip completes. The user can undo the queued accept via
// POST /api/rips/{id}/undo-accept.
func (s *Server) handleAccept(w http.ResponseWriter, r *http.Request) {
	id, err := strconv.Atoi(r.PathValue("id"))
	if err != nil {
		http.Error(w, "invalid rip ID", http.StatusBadRequest)
		return
	}

	// Parse optional search body (from manual TMDb search via selectSearchResult)
	var body struct {
		TMDBID    int    `json:"tmdb_id"`
		Title     string `json:"title"`
		Year      int    `json:"year"`
		Poster    string `json:"poster"`
		IsTV      bool   `json:"is_tv"`
		TvSeason  int    `json:"tv_season"`
		TvEpisode int    `json:"tv_episode"`
	}
	if !decodeJSON(w, r, &body, false) {
		return
	}

	// N1: same bounds as accept-title/accept-drive — garbage season/episode
	// must never reach the move layer.
	if body.TvSeason < 0 || body.TvSeason > maxSeasonEpisode ||
		body.TvEpisode < 0 || body.TvEpisode > maxSeasonEpisode {
		http.Error(w, "tv_season and tv_episode must be between 0 and 10000", http.StatusBadRequest)
		return
	}

	s.mu.Lock()
	session, ok := s.rips[id]
	if !ok {
		s.mu.Unlock()
		http.Error(w, "rip not found", http.StatusNotFound)
		return
	}

	// If body contains a title, use it as the naming match
	if body.Title != "" {
		session.TMDB = &naming.Result{
			ID:     body.TMDBID,
			Title:  body.Title,
			Year:   body.Year,
			Poster: body.Poster,
			IsTV:   body.IsTV,
		}
	}

	if session.TMDB == nil {
		s.mu.Unlock()
		http.Error(w, "no naming suggestion available", http.StatusBadRequest)
		return
	}

	// Handle different rip states
	switch session.State {
	case string(StateAccepted):
		s.mu.Unlock()
		http.Error(w, "already accepted", http.StatusConflict)
		return
	case string(StateError):
		s.mu.Unlock()
		http.Error(w, "rip failed, cannot accept", http.StatusConflict)
		return
	case string(StateQueued):
		// A second accept while the rip is still running must update the
		// queued request, not move files out from under the ripper.
		if body.Title != "" {
			session.PendingAccept = &PendingAccept{
				TMDB:    session.TMDB,
				Source:  body.Title,
				Season:  body.TvSeason,
				Episode: body.TvEpisode,
			}
		}
		s.mu.Unlock()
		w.WriteHeader(http.StatusAccepted)
		writeJSON(w, map[string]string{
			"status":  "queued",
			"message": "Accept updated, will execute when rip completes",
		})
		return
	case string(StateRipping):
		// Queue the accept for deferred execution
		session.PendingAccept = &PendingAccept{
			TMDB:    session.TMDB,
			Source:  body.Title,
			Season:  body.TvSeason,
			Episode: body.TvEpisode,
		}
		session.State = string(StateQueued)
		s.mu.Unlock()
		w.WriteHeader(http.StatusAccepted)
		writeJSON(w, map[string]string{
			"status":  "queued",
			"message": "Accept queued, will execute when rip completes",
		})
		return
	}

	// session.State == "complete" — execute immediately
	tmdbResult := session.TMDB
	title := tmdbResult.Title
	year := tmdbResult.Year
	tmdbID := tmdbResult.ID
	ripDir := session.OutputDir
	drivePath := session.DrivePath
	session.PendingAccept = nil // clear any pending accept
	// Snapshot MoviesDir under lock to avoid race with concurrent PUT
	moviesDir := s.cfg.MoviesDir
	discName := session.DiscName
	tvSeason := session.SeasonNumber
	tvEpisode := session.EpisodeNumber
	mt := session.MediaType
	// N1: an explicit position on the accept request overrides the value
	// classified/filled during the rip. Without one, a TV match (TMDb or
	// request) still forces the TV layout — a single-episode disc fools
	// classifyRip's size heuristic into "movie".
	effectiveTV := body.IsTV || body.TvSeason > 0 || (tmdbResult != nil && tmdbResult.IsTV)
	if effectiveTV {
		if tvSeason <= 0 {
			tvSeason = 1
		}
		if body.TvSeason > 0 {
			tvSeason = body.TvSeason
		}
		if tvEpisode <= 0 {
			tvEpisode = 1
		}
		if body.TvEpisode > 0 {
			tvEpisode = body.TvEpisode
		}
		session.SeasonNumber = tvSeason
		session.EpisodeNumber = tvEpisode
	}
	s.mu.Unlock()

	logger.Info("Rip accepted", "id", id, "title", title, "year", year, "season", tvSeason, "episode", tvEpisode)

	// Serialise against a concurrent accept-title / auto-execute on this same
	// session so two moves cannot race over the rip directory or claim the same
	// destination folder.
	mu := session.acceptLock()
	mu.Lock()
	defer mu.Unlock()

	// Move to library
	result, err := move.Move(ripDir, moviesDir, title, strconv.Itoa(year), tvSeason, tvEpisode)
	if err != nil {
		failed := 0
		if result != nil {
			failed = result.FailedExtras
		}
		logger.Error("Move failed", "title", title, "failed_extras", failed, "error", err)
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	s.mu.Lock()
	session.State = string(StateAccepted)
	session.OutputPath = result.Destination
	session.CompletedAt = time.Now()
	s.mu.Unlock()

	movedAt := time.Now().UTC().Format(time.RFC3339)
	if len(result.Episodes) > 0 {
		// N1: a TV move landed one file per episode; record each so the
		// series next-up advances over all of them.
		for _, me := range result.Episodes {
			s.recordRipEntry(library.Entry{
				OutputPath: me.Path,
				DiscName:   discName,
				Title:      title,
				Year:       year,
				MediaType:  "tv",
				Season:     tvSeason,
				Episode:    me.Episode,
				TMDBID:     tmdbID,
				MovedAt:    movedAt,
			})
		}
	} else {
		s.recordRipEntry(library.Entry{
			OutputPath: result.MainFile,
			DiscName:   discName,
			Title:      title,
			Year:       year,
			MediaType:  ripMediaType(tvSeason, resolveMediaType(mt, tmdbResult)),
			Season:     tvSeason,
			Episode:    tvEpisode,
			TMDBID:     tmdbID,
			MovedAt:    movedAt,
		})
	}

	logger.Info("Moved to", "destination", result.Destination)

	s.sseMgr.Publish(sse.Event{
		Type:    "rip_accepted",
		Drive:   drivePath,
		Message: fmt.Sprintf("Moved to %s", result.Destination),
	})
	s.maybeAutoEject(drivePath, discName)

	writeJSON(w, result)
}

// handleAcceptTitle accepts a single title from a rip and moves it to the library.
func (s *Server) handleAcceptTitle(w http.ResponseWriter, r *http.Request) {
	id, err := strconv.Atoi(r.PathValue("id"))
	if err != nil {
		http.Error(w, "invalid rip ID", http.StatusBadRequest)
		return
	}

	var body struct {
		TitleName string         `json:"title_name"` // the title name (from TMDb or original)
		Index     *int           `json:"index,omitempty"`
		Year      int            `json:"year"`
		IsMain    bool           `json:"is_main"`
		TvSeason  int            `json:"tv_season,omitempty"`
		TvEpisode int            `json:"tv_episode,omitempty"`
		TMDB      *naming.Result `json:"tmdb,omitempty"`
	}
	if !decodeJSON(w, r, &body, true) {
		return
	}

	// M8: 0 means "not TV" (movie); negative or absurd values used to reach
	// MoveTitleFile verbatim, producing Season -1/ paths and S-1E-3 names.
	if body.TvSeason < 0 || body.TvSeason > maxSeasonEpisode ||
		body.TvEpisode < 0 || body.TvEpisode > maxSeasonEpisode {
		logger.Info("accept-title rejected: season/episode out of bounds",
			"rip_id", id, "tv_season", body.TvSeason, "tv_episode", body.TvEpisode)
		http.Error(w, "tv_season and tv_episode must be between 0 and 10000", http.StatusBadRequest)
		return
	}

	s.mu.Lock()
	session, ok := s.rips[id]
	if !ok {
		s.mu.Unlock()
		http.Error(w, "rip not found", http.StatusNotFound)
		return
	}

	// Prefer the clicked row's index: matching by name alone can pick the
	// wrong title when two titles share a base file name. The name match is
	// kept as a fallback for old cached frontends that omit the index.
	var titleIdx int
	found := false
	if body.Index != nil {
		i := *body.Index
		if i < 0 || i >= len(session.Titles) {
			s.mu.Unlock()
			http.Error(w, "title index out of range", http.StatusNotFound)
			return
		}
		if session.Titles[i].State != "pending" {
			s.mu.Unlock()
			http.Error(w, "title not found or already accepted", http.StatusNotFound)
			return
		}
		titleIdx = i
		found = true
	} else {
		for i, t := range session.Titles {
			if t.SourceFile == body.TitleName && t.State == "pending" {
				titleIdx = i
				found = true
				break
			}
		}
	}
	if !found {
		s.mu.Unlock()
		http.Error(w, "title not found or already accepted", http.StatusNotFound)
		return
	}

	title := session.Titles[titleIdx]
	if body.Year == 0 {
		body.Year = title.Year
	}
	if body.TMDB != nil {
		title.TMDB = body.TMDB
		session.TMDB = body.TMDB
	}

	ripDir := session.OutputDir
	session.Titles[titleIdx].State = "accepting"
	// Snapshot MoviesDir under lock to avoid race with concurrent PUT
	moviesDir := s.cfg.MoviesDir
	discName := session.DiscName
	var titleTMDBID, sessionTMDBID int
	if title.TMDB != nil {
		titleTMDBID = title.TMDB.ID
	}
	if session.TMDB != nil {
		sessionTMDBID = session.TMDB.ID
	}
	s.mu.Unlock()

	logger.Info("Accepting title", "title", body.TitleName, "rip_id", id)

	// Serialise against concurrent accepts on this session (see handleAccept).
	mu := session.acceptLock()
	mu.Lock()
	defer mu.Unlock()

	// Move exactly the selected title's source file; MoveTitleFile never
	// re-picks by name/size heuristics.
	result, err := move.MoveTitleFile(ripDir, moviesDir, title.AcceptedName, body.Year, title.SourceFile, title.IsMainFile, body.TvSeason, body.TvEpisode)
	if err != nil {
		logger.Error("Move failed", "title", body.TitleName, "error", err)
		s.mu.Lock()
		session.Titles[titleIdx].State = "error"
		session.Titles[titleIdx].Error = err.Error()
		s.mu.Unlock()
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	s.mu.Lock()
	session.Titles[titleIdx].State = "accepted"
	session.Titles[titleIdx].Destination = result.Dest
	// Mark session accepted when all titles have been moved
	allAccepted := true
	for _, t := range session.Titles {
		if t.State != "accepted" {
			allAccepted = false
			break
		}
	}
	if allAccepted {
		session.State = string(StateAccepted)
	}
	s.mu.Unlock()

	recordTitle := title.AcceptedName
	if recordTitle == "" {
		recordTitle = body.TitleName
	}
	tmdbID := titleTMDBID
	if tmdbID == 0 {
		tmdbID = sessionTMDBID
	}
	mediaType := "movie"
	if body.TvSeason > 0 {
		mediaType = "tv"
	}
	s.recordRipEntry(library.Entry{
		OutputPath: result.Dest,
		DiscName:   discName,
		Title:      recordTitle,
		Year:       body.Year,
		MediaType:  mediaType,
		Season:     body.TvSeason,
		Episode:    body.TvEpisode,
		TMDBID:     tmdbID,
		MovedAt:    time.Now().UTC().Format(time.RFC3339),
	})

	logger.Info("Moved title to", "destination", result.Dest)
	if allAccepted {
		s.maybeAutoEject(session.DrivePath, discName)
	}

	writeJSON(w, map[string]string{"status": "ok", "destination": result.Dest})
}

// handleUndoAccept cancels a queued accept for a rip that is still in progress.
func (s *Server) handleUndoAccept(w http.ResponseWriter, r *http.Request) {
	id, err := strconv.Atoi(r.PathValue("id"))
	if err != nil {
		http.Error(w, "invalid rip ID", http.StatusBadRequest)
		return
	}

	s.mu.Lock()
	session, ok := s.rips[id]
	if !ok {
		s.mu.Unlock()
		http.Error(w, "rip not found", http.StatusNotFound)
		return
	}
	if session.State != string(StateQueued) || session.PendingAccept == nil {
		s.mu.Unlock()
		http.Error(w, "no queued accept to undo", http.StatusConflict)
		return
	}
	session.PendingAccept = nil
	session.State = string(StateRipping)
	s.mu.Unlock()

	logger.Info("Undo accept", "id", id, "message", "queued accept cancelled")
	writeJSON(w, map[string]string{"status": "ok", "message": "Accept cancelled, rip will continue"})
}

// handleSearch allows manual TMDb search.
func (s *Server) handleSearch(w http.ResponseWriter, r *http.Request) {
	if s.namer == nil {
		http.Error(w, "naming not configured", http.StatusServiceUnavailable)
		return
	}

	// Parse query parameter
	query := r.URL.Query().Get("q")
	if query == "" {
		http.Error(w, "missing query parameter 'q'", http.StatusBadRequest)
		return
	}

	logger.Debug("Manual search", "query", query)

	results, err := s.namer.Search(r.Context(), query)
	if err != nil {
		logger.Error("Manual search failed", "error", err)
		// Upstream failure text (wrapped URLs, status bodies) must never reach
		// the browser; the full error is in the server log above.
		http.Error(w, "search failed", http.StatusInternalServerError)
		return
	}

	writeJSON(w, results)
}

// handleGetConfig returns the current config.
func (s *Server) handleGetConfig(w http.ResponseWriter, r *http.Request) {
	s.mu.Lock()
	// Don't expose the API key
	cfg := *s.cfg
	cfg.TMDBKey = ""
	s.mu.Unlock()
	writeJSON(w, cfg)
}

// checkDirWritable probes a candidate directory's writability; seam for
// tests (handlePutConfig, L13).
var checkDirWritable = realCheckDirWritable

// realCheckDirWritable creates dir when absent and verifies the server
// process can write a probe file into it. Lazy creation matches the rip
// start path (temp dirs are made on demand), so "absent but creatable" is
// accepted and only a real permission/existence failure is rejected.
func realCheckDirWritable(dir string) error {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	probe := filepath.Join(dir, ".riply-write-check")
	f, err := os.Create(probe)
	if err != nil {
		return err
	}
	f.Close()
	_ = os.Remove(probe)
	return nil
}

// handlePutConfig updates config values and persists to disk. Invalid
// values are rejected with 400 before anything changes (L13).
func (s *Server) handlePutConfig(w http.ResponseWriter, r *http.Request) {
	var updates map[string]interface{}
	if !decodeJSON(w, r, &updates, true) {
		return
	}

	// L13: reject invalid values before touching the live config — all or
	// nothing. http_port out of range previously persisted and only failed
	// at the next restart's bind; unwritable dirs previously surfaced much
	// later as mid-rip write errors.
	if raw, ok := updates["http_port"]; ok {
		n, isNum := raw.(float64)
		if !isNum || n < 1 || n > 65535 {
			http.Error(w, fmt.Sprintf("http_port must be an integer between 1 and 65535, got %v", raw), http.StatusBadRequest)
			return
		}
	}
	for _, key := range []string{"movies_dir", "temp_dir"} {
		raw, ok := updates[key]
		if !ok {
			continue
		}
		v, isStr := raw.(string)
		if !isStr || v == "" {
			http.Error(w, fmt.Sprintf("%s must be a non-empty string", key), http.StatusBadRequest)
			return
		}
		// Expand "~" to the home directory before the writability check so
		// both the check and the assignment below act on the real path —
		// a raw "~/Movies" would otherwise create a literal "~" dir.
		v = config.ExpandHome(v)
		updates[key] = v
		if err := checkDirWritable(v); err != nil {
			http.Error(w, fmt.Sprintf("%s %q is not usable: %v", key, v, err), http.StatusBadRequest)
			return
		}
	}
	s.mu.Lock()
	if v, ok := updates["movies_dir"].(string); ok && v != "" {
		s.cfg.MoviesDir = v
	}
	if v, ok := updates["temp_dir"].(string); ok && v != "" {
		s.cfg.TempDir = v
	}
	newKey, keyPresent := updates["tmdb_api_key"].(string)
	if keyPresent {
		s.cfg.TMDBKey = newKey
	}
	if v, ok := updates["http_port"].(float64); ok && v > 0 {
		s.cfg.HTTPPort = int(v)
	}
	if v, ok := updates["http_host"].(string); ok && v != "" {
		s.cfg.HTTPHost = v
	}
	if v, ok := updates["auto_sequence"].(bool); ok {
		s.cfg.AutoSequence = v
	}
	if v, ok := updates["autorip"].(bool); ok {
		s.cfg.AutoRip = v
	}
	if v, ok := updates["min_title_length"].(float64); ok {
		n := int(v)
		if n < 0 {
			n = 0
		}
		s.cfg.MinTitleLength = n
	}
	// Snapshot the config for saving and response, release lock before I/O
	cfgCopy := *s.cfg

	s.mu.Unlock()

	// Propagate a present key (including an empty one, which disables naming)
	// to the live client so Settings takes effect without a restart.
	if keyPresent && s.namer != nil {
		s.namer.SetAPIKey(newKey)
		logger.Info("TMDb API key updated", "enabled", newKey != "")
	}

	// Propagate the min-title-length change to the live ripper so Settings
	// takes effect for the next rip without a restart (N2).
	if setter, ok := s.ripper.(rip.MinLengthSetter); ok {
		setter.SetMinTitleLength(cfgCopy.MinTitleLength)
	}

	// Persist to disk (outside mutex to avoid blocking other handlers)
	if s.cfgPath != "" {
		if err := cfgCopy.Save(s.cfgPath); err != nil {
			logger.Error("Failed to save config", "error", err)
			http.Error(w, "config saved in memory but not to disk", http.StatusInternalServerError)
			return
		}
	}

	// Never echo the API key back.
	cfgCopy.TMDBKey = ""
	writeJSON(w, cfgCopy)
}

// statusResponse is the GET /api/status body: whether MakeMKV is usable.
// The browser uses it to show a read-only "install MakeMKV" hint — the
// actual install happens in a terminal (it needs sudo), so there is no
// privileged install endpoint.
type statusResponse struct {
	Version          string `json:"version"`
	MakemkvAvailable bool   `json:"makemkv_available"`
	MakemkvVersion   string `json:"makemkv_version,omitempty"`
}

// handleStatus reports backend status. Currently that is MakeMKV
// availability; IsAvailable re-probes when the verdict is stale, so a
// machine that installs MakeMKV without restarting the server is reflected
// on the next poll.
func (s *Server) handleStatus(w http.ResponseWriter, r *http.Request) {
	avail := s.ripper != nil && s.ripper.IsAvailable()
	resp := statusResponse{MakemkvAvailable: avail}
	resp.Version = version.Version
	if avail {
		resp.MakemkvVersion = s.ripper.Version()
	}
	writeJSON(w, resp)
}

// handleMetrics returns JSON metrics.
func (s *Server) handleMetrics(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.Write(s.metrics.ToJSON())
}

// sseKeepaliveInterval bounds idle time between frames. Reverse proxies and
// intermediate NATs drop quiet connections; a comment frame (": ...") is
// ignored by EventSource but refreshes every hop's idle timer (L10). Package
// var so tests can shorten it.
var sseKeepaliveInterval = 25 * time.Second

// handleSSE handles Server-Sent Events connections.
func (s *Server) handleSSE(w http.ResponseWriter, r *http.Request) {
	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "SSE not supported", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")

	sub := s.sseMgr.Subscribe()
	defer s.sseMgr.Unsubscribe(sub)

	keepalive := time.NewTicker(sseKeepaliveInterval)
	defer keepalive.Stop()

	for {
		select {
		case <-r.Context().Done():
			return
		case <-keepalive.C:
			fmt.Fprint(w, ": keepalive\n\n")
			flusher.Flush()
		case event, ok := <-sub.Chan():
			if !ok {
				return
			}
			data, _ := json.Marshal(event)
			fmt.Fprintf(w, "event: %s\ndata: %s\n\n", event.Type, data)
			flusher.Flush()
		}
	}
}

// handleIndex serves the main HTML page from the embedded template.
func (s *Server) handleIndex(w http.ResponseWriter, r *http.Request) {
	data, err := web.FS.ReadFile("templates/index.html")
	if err != nil {
		logger.Error("Failed to read embedded index", "error", err)
		http.Error(w, "index unavailable", http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Write(data)
}

// writeJSON writes a JSON response.
func writeJSON(w http.ResponseWriter, v interface{}) {
	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(v); err != nil {
		logger.Error("Failed to encode JSON response", "error", err)
	}
}

// populateTitles builds the per-title list with proposed destinations.
func populateTitles(ripDir, moviesDir string, tmdb *naming.Result, mediaType string, season, episode int) []*RipTitle {
	if tmdb == nil {
		// No naming match — just list files, named after the file itself.
		return listFilesAsTitles(ripDir, mediaType, season, episode, "", 0, nil)
	}

	title := tmdb.Title
	year := tmdb.Year

	previews, err := move.PreviewTitles(ripDir, moviesDir, title, strconv.Itoa(year), mediaType, season, episode)
	if err != nil {
		// Fallback: just list files, named after the naming match.
		return listFilesAsTitles(ripDir, mediaType, season, episode, title, year, tmdb)
	}

	var titles []*RipTitle
	for _, p := range previews {
		titles = append(titles, &RipTitle{
			SourceFile:          p.SourceFile,
			SizeMB:              p.SizeMB,
			IsMainFile:          p.IsMainFile,
			AcceptedName:        title,
			Year:                year,
			TMDB:                tmdb,
			ProposedDestination: p.ProposedDestination,
			TvSeason:            p.TvSeason,
			TvEpisode:           p.TvEpisode,
			State:               "pending",
		})
	}

	return titles
}

// listFilesAsTitles lists the MKVs in ripDir as pending RipTitles. acceptedName/
// year/tmdb come from a naming match when one exists; without one each title is
// named after its own file.
func listFilesAsTitles(ripDir, mediaType string, season, episode int, acceptedName string, year int, tmdb *naming.Result) []*RipTitle {
	var titles []*RipTitle
	files, err := move.FindMKVs(ripDir)
	if err != nil || len(files) == 0 {
		return titles
	}
	biggest, err := move.BiggestFile(files)
	if err != nil {
		biggest = files[0]
	}
	for i, f := range files {
		info, err := os.Stat(f)
		if err != nil {
			continue
		}
		sizeMB := float64(info.Size()) / 1024 / 1024
		base := filepath.Base(f)
		isMain := f == biggest && mediaType == "movie"
		tvSeason, tvEpisode := 0, 0
		if season > 0 {
			tvSeason = season
			tvEpisode = episode + i
		}
		name := base
		if acceptedName != "" {
			name = acceptedName
		}
		titles = append(titles, &RipTitle{
			SourceFile:   base,
			SizeMB:       sizeMB,
			IsMainFile:   isMain,
			AcceptedName: name,
			Year:         year,
			TMDB:         tmdb,
			State:        "pending",
			TvSeason:     tvSeason,
			TvEpisode:    tvEpisode,
		})
	}
	return titles
}
