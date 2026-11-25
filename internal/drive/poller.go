package drive

import (
	"context"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"riply/internal/logger"
)

const pollInterval = 2 * time.Second

// removalGraceScans is how many consecutive scans a path may be absent
// before it is reported removed (pollInterval × 3 = 6 s). USB optical
// drives can re-enumerate for a few seconds during link resets.
const removalGraceScans = 3

// pollState tracks per-drive discovery state for removal debouncing and
// hotswap detection.
type pollState struct {
	discPresent  bool
	missingScans int
	seq          int64 // last media capacity (512-byte sectors) seen
	seqKnown     bool  // seq was probed successfully at least once
}

// probeResult is one drive's observation for a single scan tick.
type probeResult struct {
	path       string
	state      mediaState
	seq        int64
	seqKnown   bool
	mediaBytes int64
}

// findDriveFiles is a seam for poller unit tests; production enumerates /dev.
var findDriveFiles = findDriveDevices

// probeMediaStatus/probeMediaSize are seams for poller unit tests; production
// uses the real hardware probes.
var (
	probeMediaStatus = mediaStatus
	probeMediaSize   = defaultMediaSize
)

// defaultMediaSize reads the kernel-observed media capacity from sysfs. The
// CDROM_DRIVE_STATUS open in mediaStatus forces a media revalidation on every
// tick, so the size reflects the disc actually inserted now. A swap between
// two discs of identical capacity inside a single 2 s window is not
// distinguishable this way; that case falls back to the physical tray-open
// presence flap on the next tick.
func defaultMediaSize(devPath string) (int64, bool) {
	name := extractDeviceName(devPath)
	data, err := os.ReadFile("/sys/block/" + name + "/size")
	if err != nil {
		return 0, false
	}
	size, err := strconv.ParseInt(strings.TrimSpace(string(data)), 10, 64)
	if err != nil {
		return 0, false
	}
	return size, true
}

// probeConcurrency bounds how many drives are probed at once (L11). Optical
// drive probes (CDROM_DRIVE_STATUS ioctl + sysfs read) are I/O-bound and a
// bad disc can spin one drive's ioctl up for seconds; serial probing let one
// slow drive delay every other drive's state change until it finished.
const probeConcurrency = 4

// scanProbeWait bounds how long one scan tick waits for probe results. A
// drive still probing when the deadline passes is skipped this tick (its
// in-flight guard prevents goroutine pile-up); it is re-probed once the
// stalled probe returns. Package var as a test seam.
var scanProbeWait = pollInterval

// Poller detects drive and disc changes by polling the hardware.
type Poller struct {
	mu       sync.Mutex
	drives   map[string]*pollState // devPath -> per-drive state; key absent = drive gone
	inflight map[string]bool       // devPath -> probe goroutine running (L11)
	cancel   context.CancelFunc
}

// NewPoller creates a new Poller.
func NewPoller() *Poller {
	return &Poller{drives: make(map[string]*pollState), inflight: make(map[string]bool)}
}

// Run starts the polling loop. Blocks until ctx is cancelled.
func (p *Poller) Run(ctx context.Context, ch chan<- DiscEvent) {
	ctx, cancel := context.WithCancel(ctx)
	p.mu.Lock()
	p.cancel = cancel
	p.mu.Unlock()
	defer p.Stop()

	p.scan(ctx, ch)

	ticker := time.NewTicker(pollInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			p.scan(ctx, ch)
		}
	}
}

// Stop signals the poller to stop. Safe to call before Run or multiple times.
func (p *Poller) Stop() {
	p.mu.Lock()
	cancel := p.cancel
	p.cancel = nil
	p.mu.Unlock()
	if cancel != nil {
		cancel()
	}
}

// dispatchProbes starts one probe per drive concurrently (bounded by
// probeConcurrency) and returns a channel of results plus the number of
// probes dispatched. Drives whose probe goroutine is still in flight from an
// earlier tick (a wedged ioctl) are skipped rather than relaunched, so a
// stalled drive cannot spawn unbounded goroutines. Results arrive in
// completion order; scan applies them incrementally and applies its own
// scanProbeWait deadline.
func (p *Poller) dispatchProbes(ctx context.Context, files []driveFile) (<-chan probeResult, int) {
	out := make(chan probeResult, len(files))

	// work carries drives that actually need probing this tick; dispatched
	// counts them so scan knows how many results to expect.
	work := make(chan driveFile, len(files))
	dispatched := 0
	for _, f := range files {
		p.mu.Lock()
		if p.inflight[f.Path] {
			p.mu.Unlock()
			continue // already probing from an earlier tick; don't pile up
		}
		p.inflight[f.Path] = true
		p.mu.Unlock()
		work <- f
		dispatched++
	}

	// Bounded worker pool; out is buffered for dispatched results so a
	// worker never blocks on send even if scan stops reading early — a wedged
	// drive blocks in the probe itself, not the send, and clears its
	// in-flight flag when the probe finally returns.
	for range probeConcurrency {
		go func() {
			for f := range work {
				state := probeMediaStatus(f.Path)
				sectors, seqKnown := probeMediaSize(f.Path)
				var mediaBytes int64
				if seqKnown {
					mediaBytes = sectors * 512 // sysfs "size" is in 512-byte sectors
				}
				p.mu.Lock()
				delete(p.inflight, f.Path)
				p.mu.Unlock()
				out <- probeResult{path: f.Path, state: state, seq: sectors, seqKnown: seqKnown, mediaBytes: mediaBytes}
			}
		}()
	}
	close(work)
	return out, dispatched
}

// scan collects drive and media state changes and emits DiscEvents.
func (p *Poller) scan(ctx context.Context, ch chan<- DiscEvent) {
	files, err := findDriveFiles()
	if err != nil {
		logger.Debug("Drive scan failed", "error", err)
		return
	}

	// Probe drives concurrently (L11): one drive's ioctl can block for seconds
	// on bad media, and serial probing let that stall delay every other
	// drive's insert/eject detection for the whole tick. Results are consumed
	// incrementally so a fast drive's event is emitted immediately; scan
	// returns once all dispatched probes complete or scanProbeWait elapses,
	// whichever comes first. A drive still probing at the deadline is simply
	// not observed this tick (flagged in-flight, so not relaunched) and is
	// re-checked on a later tick once its probe returns.
	resultsCh, dispatched := p.dispatchProbes(ctx, files)

	// A drive exists on the bus iff its device node is in files (probe
	// results can be partial now that probing is concurrent + bounded, so
	// results must NOT decide removal — only device-node presence does).
	present := make(map[string]bool, len(files))
	for _, f := range files {
		present[f.Path] = true
	}

	now := time.Now()

	// Removal pass first: independent of probe timing, keyed only on device
	// nodes that disappeared this scan. Emitted before any probe result so a
	// wedged drive's siblings aren't gated behind the wait below.
	var removed []DiscEvent
	p.mu.Lock()
	for path, st := range p.drives {
		if present[path] {
			st.missingScans = 0
			continue
		}
		st.missingScans++
		if st.missingScans >= removalGraceScans {
			delete(p.drives, path)
			removed = append(removed, DiscEvent{Path: path, EventType: DriveRemoved, Time: now})
		}
	}
	p.mu.Unlock()
	sendEvents(ctx, ch, removed)

	timer := time.NewTimer(scanProbeWait)
	defer timer.Stop()
	got := 0
	for got < dispatched {
		select {
		case r := <-resultsCh:
			got++
			sendEvents(ctx, ch, p.applyProbeResult(r, now))
		case <-timer.C:
			return
		case <-ctx.Done():
			return
		}
	}
}

// applyProbeResult updates one drive's poll state under the lock and returns
// the state-change events to emit. mediaUnknown leaves prior state untouched.
func (p *Poller) applyProbeResult(r probeResult, now time.Time) []DiscEvent {
	var events []DiscEvent
	p.mu.Lock()
	defer p.mu.Unlock()

	st, exists := p.drives[r.path]
	if !exists {
		st = &pollState{}
		p.drives[r.path] = st
		events = append(events, DiscEvent{Path: r.path, EventType: DriveDiscovered, Time: now})
	}
	switch r.state {
	case mediaUnknown:
		// No event; keep stored state.
	case mediaPresent:
		if !st.discPresent {
			events = append(events, DiscEvent{Path: r.path, EventType: DiscInserted, Time: now, MediaSize: r.mediaBytes, SizeKnown: r.seqKnown})
			st.discPresent = true
		} else if st.seqKnown && r.seqKnown && st.seq != r.seq {
			// M5: media identity changed while presence never went away —
			// eject+insert completed inside one poll window. Emit an insert
			// so the event loop re-resolves the disc name instead of leaving
			// a stale-named ready card.
			logger.Info("Media swap detected", "path", r.path,
				"old_sectors", st.seq, "new_sectors", r.seq)
			events = append(events, DiscEvent{Path: r.path, EventType: DiscInserted, Time: now, MediaSize: r.mediaBytes, SizeKnown: r.seqKnown})
		}
		st.seq = r.seq
		st.seqKnown = r.seqKnown
	case mediaAbsent:
		if st.discPresent {
			events = append(events, DiscEvent{Path: r.path, EventType: DiscEjected, Time: now})
			st.discPresent = false
		}
		st.seq = 0
		st.seqKnown = false
	}
	return events
}

// sendEvents delivers events, never blocking past context cancellation.
func sendEvents(ctx context.Context, ch chan<- DiscEvent, events []DiscEvent) {
	for _, e := range events {
		select {
		case ch <- e:
		case <-ctx.Done():
			return
		}
	}
}
