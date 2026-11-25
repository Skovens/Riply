package drive

import (
	"os"
	"regexp"
	"sort"
	"strings"
	"sync"

	"riply/internal/logger"
)

// State represents the current state of a drive.
type State string

const (
	StateNoDisc   State = "no_disc"
	StateLoading  State = "loading"
	StateReady    State = "ready"
	StateRipping  State = "ripping"
	StateComplete State = "complete"
	StateError    State = "error"
)

// Drive represents a DVD/Blu-ray drive.
type Drive struct {
	Name                string `json:"name"`
	Path                string `json:"path"`
	State               State  `json:"state"`
	DiscName            string `json:"disc_name,omitempty"`      // Raw MakeMKV disc title
	CanonicalName       string `json:"canonical_name,omitempty"` // TMDb/manual canonical name
	CanonicalYear       int    `json:"canonical_year,omitempty"` // TMDb/year
	CanonicalID         int    `json:"canonical_id,omitempty"`   // TMDb ID of the canonical match
	AutoNamingEnabled   bool   `json:"auto_naming_enabled"`      // toggle for auto-naming on disc insert
	AutoSequenceEnabled bool   `json:"auto_sequence_enabled"`    // global default true; per-drive override
	AutoRipEnabled      bool   `json:"autorip_enabled"`          // global default true; per-drive override
	HasDisc             bool   `json:"has_disc"`                 // whether a disc is currently present
	AcceptedName        string `json:"accepted_name,omitempty"`  // name accepted before rip
	AcceptedYear        int    `json:"accepted_year,omitempty"`  // year of accepted year
	Poster              string `json:"poster,omitempty"`         // TMDb poster URL from the naming match
	MediaSize           int64  `json:"media_size,omitempty"`     // disc capacity in bytes; 0 when unknown
}

// Scanner handles drive discovery and state management.
// It consumes DiscEvents and updates drive state accordingly.
type Scanner struct {
	mu        sync.RWMutex
	drives    map[string]*Drive
	insertGen map[string]uint64 // bumped on every DiscInserted; CAS for resolve
}

// New creates a new Scanner.
func New() *Scanner {
	return &Scanner{drives: make(map[string]*Drive), insertGen: make(map[string]uint64)}
}

// ApplyEvent updates drive state for one detected event.
func (s *Scanner) ApplyEvent(event DiscEvent) {
	s.handleEvent(event)
}

// handleEvent processes a single DiscEvent and updates drive state.
func (s *Scanner) handleEvent(event DiscEvent) {
	s.mu.Lock()
	defer s.mu.Unlock()

	switch event.EventType {
	case DriveDiscovered:
		if _, exists := s.drives[event.Path]; !exists {
			s.drives[event.Path] = &Drive{
				Name:                formatDriveName(event.Path),
				Path:                event.Path,
				State:               StateNoDisc,
				AutoNamingEnabled:   true,
				AutoSequenceEnabled: true,
				AutoRipEnabled:      true,
			}
			logger.Info("Drive discovered", "path", event.Path)
		}
	case DriveRemoved:
		if _, exists := s.drives[event.Path]; exists {
			delete(s.drives, event.Path)
			logger.Info("Drive removed", "path", event.Path)
		}
	case DiscInserted:
		if d, exists := s.drives[event.Path]; exists {
			s.insertGen[event.Path]++
			d.MediaSize = event.MediaSize
			switch d.State {
			case StateNoDisc:
				d.State = StateReady
				d.HasDisc = true
				logger.Info("Disc inserted", "path", event.Path)
			case StateRipping:
				// A swap detected while a rip runs must not demote the drive;
				// the rip consumer will settle the state.
				d.HasDisc = true
				logger.Info("DiscInserted while ripping; drive state kept", "path", event.Path)
			default:
				// Ready/Loading/Complete/Error + hardware-present: either an
				// insert we never saw an eject for, or a hotswap the poller
				// detected via the media-change count. Clear the stale name —
				// the event loop re-resolves — instead of silently dropping it.
				logger.Info("Disc inserted while drive not empty; re-resolving",
					"path", event.Path, "previous_state", d.State, "previous_disc_name", d.DiscName)
				d.DiscName = ""
				d.CanonicalName = ""
				d.CanonicalYear = 0
				d.CanonicalID = 0
				d.Poster = ""
				d.HasDisc = true
				d.State = StateReady
			}
		}
	case DiscEjected:
		if d, exists := s.drives[event.Path]; exists {
			s.insertGen[event.Path]++
			d.State = StateNoDisc
			d.DiscName = ""
			d.CanonicalName = ""
			d.CanonicalYear = 0
			d.HasDisc = false
			d.MediaSize = 0
			logger.Info("Disc ejected", "path", event.Path)
		}
	}
}

// InsertGeneration returns the number of insert events seen for path. The
// event loop snapshots it before resolving a disc name; CommitReady uses it as
// a compare-and-set token so a resolve result only lands if no eject/insert
// happened while MakeMKV was running (M6: closes the re-check→update TOCTOU).
func (s *Scanner) InsertGeneration(path string) uint64 {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.insertGen[path]
}

// CommitReady writes (StateReady, discName) for path only if generation still
// matches, a disc is present, and the drive was not ripped or cleared in the
// meantime. Returns false when the update was dropped — a stale resolve must
// never overwrite the current state.
func (s *Scanner) CommitReady(path, discName string, gen uint64) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	d, ok := s.drives[path]
	if !ok {
		logger.Info("Stale ready dropped: drive gone", "path", path)
		return false
	}
	if s.insertGen[path] != gen || !d.HasDisc || d.State == StateRipping {
		logger.Info("Stale ready dropped", "path", path, "gen", gen,
			"current_gen", s.insertGen[path], "state", d.State, "has_disc", d.HasDisc)
		return false
	}
	d.State = StateReady
	d.DiscName = discName
	return true
}

// Snapshot returns a copy of the drive at path.
func (s *Scanner) Snapshot(path string) (Drive, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	d, ok := s.drives[path]
	if !ok {
		return Drive{}, false
	}
	return *d, true
}

// UpdateState updates the state of a drive.
// M5 guard: a drive whose disc is gone (poller DiscEjected, or API eject via
// UpdateState(StateNoDisc)) must not be written Complete/Ready by the late
// rip consumer — the disc is gone; the name would be fiction.
func (s *Scanner) UpdateState(path string, state State, discName string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	d, ok := s.drives[path]
	if !ok {
		logger.Info("UpdateState: drive not found", "path", path)
		return
	}
	if (state == StateComplete || state == StateReady) && !d.HasDisc {
		logger.Info("UpdateState dropped: no disc on drive (guard: eject-during-rip)",
			"path", path, "attempted_state", state)
		return
	}
	logger.Info("UpdateState", "path", path, "state", state, "disc_name", discName, "previous_state", d.State)
	if state == StateNoDisc {
		d.State = state
		d.HasDisc = false
		d.DiscName = ""
		d.CanonicalName = ""
		d.CanonicalYear = 0
		d.MediaSize = 0
		return
	}
	d.State = state
	if discName != "" {
		d.DiscName = discName
	}
}

// BeginLoading transitions a drive to StateLoading atomically, refusing while
// a rip is in progress (a hotswap DiscInserted on a ripping drive must not
// demote the rip card) or when the disc is gone.
func (s *Scanner) BeginLoading(path string) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	d, ok := s.drives[path]
	if !ok {
		logger.Info("BeginLoading: drive not found", "path", path)
		return false
	}
	if d.State == StateRipping {
		logger.Info("BeginLoading refused: drive ripping", "path", path)
		return false
	}
	if !d.HasDisc {
		logger.Info("BeginLoading refused: no disc", "path", path)
		return false
	}
	d.State = StateLoading
	return true
}

// All returns all drives.
func (s *Scanner) All() []Drive {
	s.mu.RLock()
	defer s.mu.RUnlock()
	result := make([]Drive, 0, len(s.drives))
	for _, d := range s.drives {
		result = append(result, *d)
	}
	sort.Slice(result, func(i, j int) bool {
		return result[i].Path < result[j].Path
	})
	return result
}

// SetAutoNaming toggles auto-naming for a drive by path.
func (s *Scanner) SetAutoNaming(path string, enabled bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if d, ok := s.drives[path]; ok {
		d.AutoNamingEnabled = enabled
	}
}

// SetAutoSequence toggles Auto-Sequence for a drive by path.
func (s *Scanner) SetAutoSequence(path string, enabled bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if d, ok := s.drives[path]; ok {
		d.AutoSequenceEnabled = enabled
	}
}

// SetAutoRip toggles Autorip for a drive by path.
func (s *Scanner) SetAutoRip(path string, enabled bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if d, ok := s.drives[path]; ok {
		d.AutoRipEnabled = enabled
	}
}

// driveFile represents a discovered drive device.
type driveFile struct {
	Path string
}

var srRegex = regexp.MustCompile(`^sr\d+$`)

func findDriveDevices() ([]driveFile, error) {
	entries, err := os.ReadDir("/dev")
	if err != nil {
		return nil, err
	}

	var drives []driveFile
	for _, e := range entries {
		if srRegex.MatchString(e.Name()) {
			drives = append(drives, driveFile{Path: "/dev/" + e.Name()})
		}
	}
	sort.Slice(drives, func(i, j int) bool {
		return drives[i].Path < drives[j].Path
	})
	logger.Debug("Drive scan", "count", len(drives))
	return drives, nil
}

func extractDeviceName(path string) string {
	// /dev/sr0 -> sr0
	return path[len("/dev/"):]
}

func formatDriveName(path string) string {
	base := path[len("/dev/"):] // e.g., "sr0", "sr10"
	return "Drive " + strings.TrimPrefix(base, "sr")
}
