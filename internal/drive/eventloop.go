package drive

import (
	"context"
	"fmt"
	"os/exec"
	"strings"

	"riply/internal/logger"
	"riply/internal/rip"
	"riply/internal/sse"
)

// UnknownDiscName is displayed when neither MakeMKV nor the volume label
// identifies the disc. The device path is never shown in the UI.
const UnknownDiscName = "Unknown disc"

// DiscHooks are the callbacks an EventLoop needs from the application layer:
// clearing a stale naming match, kicking off auto-naming once a disc name
// resolved, and warning when a disc was ripped before. They keep
// internal/drive free of api package dependencies.
type DiscHooks interface {
	ClearNamingMatch(drivePath string)
	StartAutoNaming(drivePath, discName string)
	DuplicateCheck(ctx context.Context, drivePath, discName string)
	AutoSequenceDefault() bool
	AutoRipDefault() bool
	MaybeStartAutoRip(drivePath, discName string)
}

// EventLoop consumes DiscEvents from a poller channel, applies them to the
// scanner, and resolves disc names asynchronously via MakeMKV with a volume
// label fallback. It runs until ctx is cancelled or eventCh closes.
type EventLoop struct {
	ctx     context.Context
	scanner *Scanner
	ripper  rip.Ripper
	sseMgr  *sse.Manager
	hooks   DiscHooks
	eventCh <-chan DiscEvent
}

// NewEventLoop creates an EventLoop bound to the given dependencies.
func NewEventLoop(ctx context.Context, scanner *Scanner, ripper rip.Ripper, sseMgr *sse.Manager, hooks DiscHooks, eventCh <-chan DiscEvent) *EventLoop {
	return &EventLoop{ctx: ctx, scanner: scanner, ripper: ripper, sseMgr: sseMgr, hooks: hooks, eventCh: eventCh}
}

// Run processes events until cancelled. Call in a goroutine.
func (l *EventLoop) Run() {
	logger.Info("Event loop started")
	for {
		select {
		case <-l.ctx.Done():
			logger.Info("Event loop exiting")
			return
		case event, ok := <-l.eventCh:
			if !ok {
				logger.Info("Event loop exiting")
				return
			}
			logger.Info("Event received", "type", event.EventType, "path", event.Path)
			l.scanner.ApplyEvent(event)

			switch event.EventType {
			case DriveDiscovered:
				l.scanner.SetAutoSequence(event.Path, l.hooks.AutoSequenceDefault())
				l.scanner.SetAutoRip(event.Path, l.hooks.AutoRipDefault())
			case DiscInserted:
				l.hooks.ClearNamingMatch(event.Path)
				l.scanner.SetAutoSequence(event.Path, l.hooks.AutoSequenceDefault())
				l.scanner.SetAutoRip(event.Path, l.hooks.AutoRipDefault())
				go l.resolveDiscName(event.Path)
			case DiscEjected:
				l.hooks.ClearNamingMatch(event.Path)
				l.sseMgr.Publish(sse.Event{
					Type:    "drive_ejected",
					Drive:   event.Path,
					Message: "Disc ejected",
				})
			}
		}
	}
}

// resolveDiscName probes the drive for a disc title (MakeMKV first, then the
// filesystem volume label), updates scanner state, starts auto-naming when a
// real name was found, and publishes drive_ready. GetDiscInfo may retry for a
// while due to MakeMKV initialization lag, so this runs in its own goroutine.
func (l *EventLoop) resolveDiscName(path string) {
	logger.Info("Disc name resolution started", "path", path)
	gen := l.scanner.InsertGeneration(path)
	logger.Debug("resolveDiscName generation", "path", path, "gen", gen)
	drives := l.scanner.All()
	logger.Info("Scanner returned drives", "count", len(drives))
	for idx, d := range drives {
		logger.Info("Drive scan", "index", idx, "path", d.Path, "state", d.State, "disc_name", d.DiscName)
		if d.Path != path {
			continue
		}
		discName := ""
		logger.Info("Found matching drive", "index", idx)
		// M5: a hotswap DiscInserted during a rip must not demote the ripping
		// card to loading; BeginLoading refuses while ripping or empty, so the
		// rip consumer keeps sole ownership of the drive state.
		if !l.scanner.BeginLoading(d.Path) {
			logger.Info("Resolve skipped: drive not loadable (ripping or empty)", "path", d.Path)
			return
		}
		l.sseMgr.Publish(sse.Event{
			Type:    "drive_loading",
			Drive:   d.Path,
			Message: "Loading disc...",
		})

		// Call GetDiscInfo via MakeMKV
		if l.ripper != nil && l.ripper.IsAvailable() {
			logger.Info("GetDiscInfo", "path", d.Path, "operation", "info")
			discInfo, err := l.ripper.GetDiscInfo(l.ctx, d.Path)
			if err == nil && discInfo.Title != "" {
				discName = discInfo.Title
				logger.Info("Disc name resolved from GetDiscInfo", "title", discName)
			} else if err != nil {
				logger.Warn("GetDiscInfo failed", "path", path, "error", err, "message", "using fallback name")
			}
		}

		// M6: the re-check below is advisory; the authoritative atomicity comes
		// from CommitReady's generation compare-and-swap, so an eject (or a
		// second insert) landing after the re-check still drops this resolve.
		current, ok := l.scanner.Snapshot(d.Path)
		if !ok || current.State != StateLoading || !current.HasDisc {
			logger.Info("Disc info discarded", "path", d.Path, "state", current.State, "has_disc", current.HasDisc, "message", "disc ejected during info lookup")
			return
		}

		// Normalize the resolved name
		resolved := strings.TrimSpace(discName)
		if resolved == "" || strings.EqualFold(resolved, "unknown") {
			resolved = discLabel(l.ctx, d.Path)
			logger.Info("Disc name fallback", "path", d.Path, "source", "volume-label", "raw", resolved)
		}
		if resolved == "" || strings.EqualFold(resolved, "unknown") ||
			resolved == "DVDVolume" || resolved == "Blu-ray Disc" {
			resolved = UnknownDiscName
			logger.Warn("Disc name unresolved", "path", d.Path, "fallback", resolved)
		}
		discName = resolved

		// Apply state atomically (M6): only lands if no eject/insert happened
		// while MakeMKV was running.
		if !l.scanner.CommitReady(d.Path, discName, gen) {
			logger.Info("Ready commit dropped: stale resolve", "path", d.Path, "disc_name", discName)
			return
		}
		if discName != UnknownDiscName {
			logger.Info("Starting auto-naming", "drive", d.Path)
			l.hooks.StartAutoNaming(d.Path, discName)
			if cur, ok := l.scanner.Snapshot(d.Path); ok && cur.AutoSequenceEnabled {
				go l.hooks.DuplicateCheck(l.ctx, d.Path, discName)
			}
		}
		logger.Info("Publishing drive_ready SSE event", "drive", d.Path, "disc_name", discName)
		l.sseMgr.Publish(sse.Event{
			Type:    "drive_ready",
			Drive:   d.Path,
			Message: fmt.Sprintf("Disc detected: %s", discName),
		})
		if discName != UnknownDiscName {
			if cur, ok := l.scanner.Snapshot(d.Path); ok && cur.AutoRipEnabled {
				go l.hooks.MaybeStartAutoRip(d.Path, discName)
			}
		}
		break
	}
	logger.Info("Disc name resolution completed", "path", path)
}

// discLabel returns the filesystem volume label for devPath, or "" when it
// cannot be read. lsblk prints one line per partition; the first non-empty
// label wins.
func discLabel(ctx context.Context, devPath string) string {
	out, err := exec.CommandContext(ctx, "lsblk", "-no", "LABEL", devPath).Output()
	if err != nil {
		return ""
	}
	for _, line := range strings.Split(string(out), "\n") {
		if label := strings.TrimSpace(line); label != "" {
			return label
		}
	}
	return ""
}
