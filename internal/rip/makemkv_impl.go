package rip

import (
	"bufio"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"os"
	"os/exec"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"riply/internal/logger"
)

// rawCaptureEnv is the env var that enables raw MakeMKV output capture.
const rawCaptureEnv = "RIPLY_MAKEMKV_LOG"

// availabilityTTL bounds how old the last successful probe may be before
// IsAvailable re-probes on demand (L3: MakeMKV installed after launch must
// not stay disabled until restart). A negative verdict retries sooner so a
// briefly wedged or just-installed binary recovers quickly without
// stampeding makemkvcon on every API call.
var (
	availabilityTTL    = 30 * time.Second
	probeRetryInterval = 5 * time.Second
	// probeTimeout bounds one makemkvcon availability probe; vars so tests
	// can exercise the timeout path quickly.
	probeTimeout = 3 * time.Second
)

// makeMKV implements the Ripper interface using the makemkvcon CLI.
type makeMKV struct {
	mu        sync.RWMutex
	version   string
	avail     bool
	lastCheck time.Time
	path      string
	rawMu     sync.Mutex
	rawFile   *os.File
	rawPath   string

	// minTitleLenSec guards the --minlength value (seconds) passed to Rip
	// invocations; written via SetMinTitleLength from the config PUT handler.
	minTitleLenMu  sync.Mutex
	minTitleLenSec int

	// probeMu serialises re-probes so concurrent IsAvailable calls share
	// one exec instead of stampeding makemkvcon.
	probeMu sync.Mutex
}

// NewMakeMKV creates a new MakeMKV ripper.
func NewMakeMKV(path string) *makeMKV {
	m := &makeMKV{path: path}
	m.check()
	return m
}

// SetMinTitleLength updates the minimum title length (seconds) passed as
// makemkvcon --minlength to subsequent rips; 0 omits the flag. Satisfies
// MinLengthSetter for runtime config changes (N2).
func (m *makeMKV) SetMinTitleLength(seconds int) {
	if seconds < 0 {
		seconds = 0
	}
	m.minTitleLenMu.Lock()
	defer m.minTitleLenMu.Unlock()
	m.minTitleLenSec = seconds
	logger.Info("MakeMKV min title length updated", "seconds", seconds)
}

func (m *makeMKV) minTitleLength() int {
	m.minTitleLenMu.Lock()
	defer m.minTitleLenMu.Unlock()
	return m.minTitleLenSec
}

// openRawCapture lazily opens the raw capture file for the given operation.
// Returns nil if RIPLY_MAKEMKV_LOG is not set or if the file fails to open.
func (m *makeMKV) openRawCapture(op string) *os.File {
	path := os.Getenv(rawCaptureEnv)
	if path == "" {
		return nil
	}

	m.rawMu.Lock()
	defer m.rawMu.Unlock()

	// Reuse the same file for the process lifetime.
	if m.rawFile != nil && m.rawPath == path {
		return m.rawFile
	}

	f, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0644)
	if err != nil {
		logger.Warn("Failed to open MakeMKV raw capture log", "path", path, "error", err)
		return nil
	}

	m.rawFile = f
	m.rawPath = path
	logger.Info("MakeMKV raw capture", "path", path, "operation", op)
	return f
}

// writeRawLine writes a single raw line to the capture file.
func (m *makeMKV) writeRawLine(line string) {
	m.rawMu.Lock()
	defer m.rawMu.Unlock()

	if m.rawFile == nil {
		return
	}
	if _, err := fmt.Fprintln(m.rawFile, line); err != nil {
		logger.Warn("Failed to write MakeMKV raw capture", "error", err)
	}
}

// probe runs makemkvcon -r info disc:0 under a 3s timeout and reports what
// it saw. The old heuristic counted a timeout as "available" — that marks a
// hanging or wedged binary usable and, paired with one-shot checking at
// startup, kept Riply disabled until restart whenever MakeMKV was installed
// or removed after launch. A timeout now means unavailable unless the
// version banner (MSG:1005) had already been printed.
func (m *makeMKV) probe() (available bool, version string) {
	path := m.path
	if path == "" {
		path = "makemkvcon"
	}
	ctx, cancel := context.WithTimeout(context.Background(), probeTimeout)
	defer cancel()

	out, err := exec.CommandContext(ctx, path, "-r", "info", "disc:0").CombinedOutput()
	s := string(out)
	if err != nil {
		if ctx.Err() != nil && strings.Contains(s, "MSG:1005") {
			// Timed out, but the banner proves the binary exists.
			return true, extractVersion(s)
		}
		return false, ""
	}
	return true, extractVersion(s)
}

func (m *makeMKV) check() {
	avail, version := m.probe()
	m.mu.Lock()
	m.avail = avail
	m.lastCheck = time.Now()
	if version != "" {
		m.version = version
	}
	m.mu.Unlock()
	if !avail {
		logger.Warn("MakeMKV probe failed", "path", m.path)
	}
}

func (m *makeMKV) IsAvailable() bool {
	m.mu.RLock()
	avail := m.avail
	since := time.Since(m.lastCheck)
	interval := availabilityTTL
	if !avail {
		interval = probeRetryInterval
	}
	m.mu.RUnlock()
	if since <= interval {
		return avail
	}
	// Stale verdict: re-probe synchronously, serialised so concurrent
	// callers share one exec instead of stampeding makemkvcon.
	m.probeMu.Lock()
	defer m.probeMu.Unlock()
	m.check()
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.avail
}

func (m *makeMKV) Version() string {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.version
}

// resolveDriveIndex maps an OS device path (e.g. /dev/sr1) to the MakeMKV
// drive index by enumerating drives with `info disc:9999` and matching the
// device column of the DRV lines. MakeMKV enumerates drives itself and the
// order is not guaranteed to follow /dev/srN numbering (USB vs SATA attach
// order, non-contiguous devices), so the numeric suffix of the path can be
// a lie (L4). On enumeration failure it falls back to the /dev/srN suffix.
func (m *makeMKV) resolveDriveIndex(ctx context.Context, devPath string) int {
	fallback := DriveIndex(devPath)

	path := m.path
	if path == "" {
		path = "makemkvcon"
	}

	listCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()

	// `info disc:9999` lists all drives without opening any specific disc.
	out, err := exec.CommandContext(listCtx, path, "-r", "info", "disc:9999").CombinedOutput()
	if err != nil && listCtx.Err() != nil {
		logger.Warn("resolveDriveIndex: enumeration timed out; falling back to /dev/srN suffix",
			"path", devPath, "fallback_index", fallback)
		return fallback
	}

	for _, line := range strings.Split(string(out), "\n") {
		matches := drvRegex.FindStringSubmatch(line)
		if matches == nil {
			continue
		}
		idx, convErr := strconv.Atoi(matches[1])
		if convErr != nil {
			continue
		}
		if matches[7] == devPath {
			if idx != fallback {
				logger.Info("resolveDriveIndex: MakeMKV index differs from /dev/srN suffix",
					"path", devPath, "mk_index", idx, "sr_suffix", fallback)
			}
			return idx
		}
	}

	logger.Warn("resolveDriveIndex: device not in makemkvcon enumeration; falling back to /dev/srN suffix",
		"path", devPath, "fallback_index", fallback)
	return fallback
}

func (m *makeMKV) GetDiscInfo(ctx context.Context, devPath string) (DiscInfo, error) {
	driveIdx := m.resolveDriveIndex(ctx, devPath)

	for attempt := 0; attempt < 5; attempt++ {
		logger.Info("GetDiscInfo attempt", "attempt", attempt+1, "drive", driveIdx)
		if attempt > 0 {
			select {
			case <-ctx.Done():
				return DiscInfo{}, nil
			case <-time.After(time.Second):
			}
		}

		info, hasMedia := m.readDiscInfo(ctx, driveIdx)
		if info.Title != "" {
			logger.Info("GetDiscInfo resolved", "drive", driveIdx, "disc", info.Title, "source", "DRV", "attempt", attempt+1)
			return info, nil
		}
		if ctx.Err() != nil {
			break
		}
		if !hasMedia {
			logger.Debug("GetDiscInfo drive not ready", "drive", driveIdx, "hasMedia", false)
			continue
		}
		break
	}

	logger.Warn("GetDiscInfo unresolved", "drive", driveIdx)
	return DiscInfo{}, nil
}

func (m *makeMKV) readDiscInfo(ctx context.Context, driveIdx int) (DiscInfo, bool) {
	path := m.path
	if path == "" {
		path = "makemkvcon"
	}

	timeoutCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()

	cmd := exec.CommandContext(timeoutCtx, path, "-r", "info", fmt.Sprintf("disc:%d", driveIdx))
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return DiscInfo{}, false
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return DiscInfo{}, false
	}
	if err := cmd.Start(); err != nil {
		logger.Error("GetDiscInfo start", "error", err)
		return DiscInfo{}, false
	}

	lines := make(chan string, 64)
	var wg sync.WaitGroup
	wg.Add(2)
	scan := func(r io.Reader) {
		defer wg.Done()
		if err := scanRobotLines(r, lines); err != nil {
			logger.Error("makemkvcon info output scanner ended with error", "error", err)
		}
	}
	go scan(stdout)
	go scan(stderr)
	go func() { wg.Wait(); close(lines) }()

	// Open raw capture file if enabled.
	rawFile := m.openRawCapture("info")

	var out strings.Builder
	var info DiscInfo
	// Scan the whole output: cancelling on title would truncate the TINFO
	// metadata the disc hash is computed from.
	for line := range lines {
		out.WriteString(line)
		out.WriteByte('\n')
		if rawFile != nil {
			m.writeRawLine(line)
		}
	}
	cmd.Wait()

	info = parseDiscInfo(out.String())
	logger.Debug("readDiscInfo result", "drive", driveIdx, "disc", info.Title, "hash", info.Hash)
	return info, hasMediaInOutput(out.String())
}

// robotMaxLine bounds a single makemkvcon robot-mode line.
const robotMaxLine = 1024 * 1024

// scanRobotLines reads makemkvcon robot-mode output line by line into out.
// A line longer than robotMaxLine is logged once (with its head) and
// discarded, then the stream continues: the old bufio.Scanner loop stopped
// consuming on token overflow (sc.Err() unchecked), which stranded the
// parser on a half-drained pipe while makemkvcon kept running — progress
// and completion then never arrived (M9). A read error other than EOF is
// returned so callers can log it.
func scanRobotLines(r io.Reader, out chan<- string) error {
	br := bufio.NewReaderSize(r, 64*1024)
	var pending []byte
	var discarding bool
	for {
		frag, isPrefix, err := br.ReadLine()
		if len(frag) > 0 {
			switch {
			case discarding:
				// Rest of an oversized line: drain without forwarding.
			case len(pending) >= robotMaxLine:
				discarding = true
				logger.Error("makemkvcon robot line exceeded limit; discarding",
					"limit_bytes", robotMaxLine, "head", truncate(string(pending), 64))
				pending = pending[:0]
			default:
				pending = append(pending, frag...)
			}
		}
		if !isPrefix {
			// End of line — also reached via an empty final fragment when
			// the line ends exactly at a buffer boundary; discarding MUST
			// reset here or the following line gets swallowed too.
			if !discarding && len(pending) > 0 {
				out <- string(pending)
			}
			pending = pending[:0]
			discarding = false
		}
		if err != nil {
			if err == io.EOF {
				return nil
			}
			return err
		}
	}
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n] + "..."
}

// hasMediaInOutput checks if any DRV line has a non-zero media flag (4th field).
func hasMediaInOutput(output string) bool {
	lines := strings.Split(output, "\n")
	for _, line := range lines {
		if m := drvRegex.FindStringSubmatch(line); m != nil {
			// m[4] is the flags field (AP_DskFsFlagXXX bitmask)
			if flags, err := strconv.Atoi(m[4]); err == nil && flags != 0 {
				return true
			}
		}
	}
	return false
}

func (m *makeMKV) Rip(ctx context.Context, devPath string, outDir string, totalBytes int64) <-chan RipProgress {
	ch := make(chan RipProgress, 16)
	driveIdx := m.resolveDriveIndex(ctx, devPath)

	// Resolve the effective byte total for progress estimation.
	const ripFactor = 0.75 // MKV output is typically 60-85% of raw media capacity; tunable
	estTotal := int64(float64(totalBytes) * ripFactor)
	if estTotal <= 0 {
		estTotal = int64(float64(mediaCapacityBytes(devPath)) * ripFactor) // sysfs fallback
	}
	logger.Info("Rip progress estimate", "dev_path", devPath, "total_bytes", totalBytes, "est_total", estTotal)
	path := m.path
	if path == "" {
		path = "makemkvcon"
	}

	// Remove any existing directory at this path (MakeMKV v2.0.0 refuses to overwrite)
	os.RemoveAll(outDir)
	// Create output directory
	if err := os.MkdirAll(outDir, 0755); err != nil {
		ch <- RipProgress{Status: "error", Message: fmt.Sprintf("mkdir: %v", err)}
		close(ch)
		return ch
	}

	// Build command: makemkvcon -r mkv disc:<idx> all <outdir>
	// mkv creates .mkv files (one per title) in a directory
	// --progress=-stdout enables PRGV/PRGC on stdout alongside MSG/DRV
	// --minlength=N (N2) skips titles shorter than N seconds; omitted when 0
	// so MakeMKV keeps its rip-everything default.
	args := []string{"--progress=-stdout"}
	if n := m.minTitleLength(); n > 0 {
		args = append(args, fmt.Sprintf("--minlength=%d", n))
	}
	args = append(args, "-r", "mkv", fmt.Sprintf("disc:%d", driveIdx), "all", outDir)

	cmd := exec.CommandContext(ctx, path, args...)
	// Pipe stdout and stderr so we can parse robot progress messages
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		ch <- RipProgress{Status: "error", Message: fmt.Sprintf("stdout pipe: %v", err)}
		close(ch)
		return ch
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		ch <- RipProgress{Status: "error", Message: fmt.Sprintf("stderr pipe: %v", err)}
		close(ch)
		return ch
	}

	if err := cmd.Start(); err != nil {
		logger.Error("MakeMKV start failed", "error", err)
		ch <- RipProgress{Status: "error", Message: fmt.Sprintf("start: %v", err)}
		close(ch)
		return ch
	}

	// Open raw capture file if enabled.
	rawFile := m.openRawCapture("rip")
	logger.Info("MakeMKV started", "pid", cmd.Process.Pid)

	// Drain stdout and stderr into a shared lines channel
	lines := make(chan string, 64*1024)
	var wg sync.WaitGroup
	wg.Add(2)
	scan := func(r io.Reader) {
		defer wg.Done()
		if err := scanRobotLines(r, lines); err != nil {
			logger.Error("makemkvcon output scanner ended with error", "error", err)
		}
	}
	go scan(stdout)
	go scan(stderr)
	go func() {
		wg.Wait()
		close(lines)
	}()

	// Consumer: parse MakeMKV events, publish progress from bytes written.
	go func() {
		defer close(ch)

		var totalTitles, savedTitles, failedTitles int
		var currentPhase string
		var prgvFrac float64
		var havePRGV bool
		var announcedTitles int
		var lastEmittedPct float64
		var prgvLogged bool
		rippingPhase := "scanning"
		// Speed readout: sample the write rate every 5s and average the last
		// 6 samples (~30s window) so the UI's MB/s and ETA don't jitter with
		// per-second disk speed fluctuation.
		var speedSamples [6]float64
		var speedSampleCount, speedSampleIdx int
		var lastSampleTime time.Time
		var lastSampleSize int64

		const rawTailLimit = 100
		rawTail := make([]string, 0, rawTailLimit)
		appendRaw := func(line string) {
			if len(rawTail) == rawTailLimit {
				copy(rawTail, rawTail[1:])
				rawTail[rawTailLimit-1] = line
			} else {
				rawTail = append(rawTail, line)
			}
		}

		ticker := time.NewTicker(time.Second)
		defer ticker.Stop()

		// N3: per-title save position from MSG:5017 (index/total), surfaced
		// on every "ripping" emit so the UI can show "Saving title 2 of 5".
		var savedIdx, savedTotal int

		// emit recomputes progress from the output directory and publishes it.
		// Percent is byte-driven when a total estimate exists; PRGV fallback otherwise.
		emit := func(now time.Time) {
			count, bytes := mkvStats(outDir)
			var pct float64
			if estTotal > 0 {
				pct = 100.0 * float64(bytes) / float64(estTotal)
			} else if havePRGV && announcedTitles > 0 {
				pct = 99 * float64(count+int(prgvFrac)) / float64(announcedTitles)
			} else if havePRGV {
				pct = 99 * prgvFrac
			} else if announcedTitles > 0 {
				pct = 99 * float64(count) / float64(announcedTitles)
			}
			if pct < 0 {
				pct = 0
			}
			if pct > 99 {
				pct = 99
			}
			// Progress never regresses
			if pct < lastEmittedPct {
				pct = lastEmittedPct
			}
			lastEmittedPct = pct
			// Speed: take a rate sample every 5s, average the last 6.
			var speed float64
			if lastSampleTime.IsZero() {
				lastSampleTime = now
				lastSampleSize = bytes
			} else if now.Sub(lastSampleTime) >= 5*time.Second {
				dt := now.Sub(lastSampleTime).Seconds()
				rate := float64(bytes-lastSampleSize) / 1024 / 1024 / dt
				if rate < 0 {
					rate = 0
				}
				speedSamples[speedSampleIdx] = rate
				speedSampleIdx = (speedSampleIdx + 1) % 6
				if speedSampleCount < 6 {
					speedSampleCount++
				}
				lastSampleTime = now
				lastSampleSize = bytes
			}
			if speedSampleCount > 0 {
				sum := 0.0
				for i := range speedSampleCount {
					sum += speedSamples[i]
				}
				speed = sum / float64(speedSampleCount)
			}
			logger.Debug("emit", "bytes", bytes, "est_total", estTotal, "pct", fmt.Sprintf("%.1f", pct), "speed", fmt.Sprintf("%.1f", speed))
			ch <- RipProgress{Status: "ripping", Title: count, Percent: pct, SpeedMBPS: speed, TotalBytes: bytes, EstimatedTotal: estTotal, Phase: currentPhase, TitleIndex: savedIdx, TitleTotal: savedTotal, RipPhase: rippingPhase}
		}

		for {
			select {
			case line, ok := <-lines:
				if !ok {
					if err := cmd.Wait(); err != nil && ctx.Err() == nil {
						logger.Error("MakeMKV exited with error", "error", err, "output_tail", strings.Join(rawTail, "\n"))
						msg := fmt.Sprintf("makemkvcon: %v", err)
						if hint := makemkvErrorHint(rawTail); hint != "" {
							msg += fmt.Sprintf(" (%s)", hint)
						}
						ch <- RipProgress{Status: "error", Message: msg}
						return
					}
					if ctx.Err() != nil {
						return
					}
					count, bytes := mkvStats(outDir)
					logger.Info("MakeMKV exited", "files", count,
						"size_mb", fmt.Sprintf("%.1f", float64(bytes)/1024/1024),
						"saved", savedTitles, "failed", failedTitles)
					if savedTitles == 0 && failedTitles > 0 {
						ch <- RipProgress{Status: "error", Message: fmt.Sprintf("No titles saved (%d failed)", failedTitles)}
						return
					}
					if count == 0 {
						logger.Error("MakeMKV produced no files",
							"saved", savedTitles, "failed", failedTitles,
							"output_tail", strings.Join(rawTail, "\n"))
						ch <- RipProgress{Status: "error", Message: fmt.Sprintf("No files produced (saved %d, failed %d)", savedTitles, failedTitles)}
						return
					}
					msg := fmt.Sprintf("Rip completed (%d file(s), %.1f MB)", count, float64(bytes)/1024/1024)
					if failedTitles > 0 {
						msg = fmt.Sprintf("Rip completed with %d failed title(s) (%d file(s), %.1f MB)", failedTitles, count, float64(bytes)/1024/1024)
					}
					ch <- RipProgress{Status: "complete", Percent: 100, Message: msg, RipPhase: "complete"}
					return
				}
				appendRaw(line)
				if rawFile != nil {
					m.writeRawLine(line)
				}
				p := parseProgress(line)
				if p == nil {
					continue
				}
				switch p.Status {
				case "phase":
					currentPhase = p.Phase
					logger.Info("MakeMKV phase", "phase", currentPhase)
					emit(time.Now())
				case "subphase":
					if p.Phase != "" {
						currentPhase = p.Phase
						emit(time.Now())
					}
				case "prgv":
					prgvFrac = p.Percent / 100
					havePRGV = true
					if !prgvLogged {
						logger.Info("MakeMKV progress stream", "current", p.Current, "max", p.Max, "percent", int(p.Percent))
						prgvLogged = true
					}
					emit(time.Now())
				case "saving":
					totalTitles = p.Title
					announcedTitles = p.Title
					rippingPhase = "saving"
					logger.Info("Saving titles", "count", totalTitles)
					emit(time.Now())
				case "save_result":
					savedTitles, failedTitles = p.Saved, p.Failed
					logger.Info("Save result", "saved", savedTitles, "failed", failedTitles)
					emit(time.Now())
				case "title_saved":
					if p.TitleIndex > 0 {
						savedIdx = p.TitleIndex
					} else {
						savedIdx++
					}
					if p.TitleTotal > 0 {
						savedTotal = p.TitleTotal
					} else if totalTitles > 0 {
						savedTotal = totalTitles
					}
					logger.Info("Title saved", "index", savedIdx, "total", savedTotal)
					emit(time.Now())
				default:
					logger.Debug("MakeMKV event", "status", p.Status, "title", p.Title, "message", p.Message)
				}
			case now := <-ticker.C:
				emit(now)
			}
		}
	}()

	return ch
}

func (m *makeMKV) Eject(ctx context.Context, devPath string) error {
	devName := devPath

	// MakeMKV v2.0.0 removed the eject command entirely.
	// Try multiple fallback methods for ejecting.

	// 1. Try system eject utility
	if err := tryEjectCmd(ctx, devName); err == nil {
		return nil
	}

	// 2. Try MakeMKV info to verify disc is gone (best effort)
	logger.Info("System eject failed", "device", devName, "message", "disc may still be loaded")
	return fmt.Errorf("eject %s: all methods failed", devName)
}

// tryEjectCmd attempts to eject using the system eject utility.
func tryEjectCmd(ctx context.Context, devName string) error {
	cmd := exec.CommandContext(ctx, "eject", devName)
	out, err := cmd.CombinedOutput()
	if err == nil {
		return nil
	}
	logger.Debug("eject utility failed", "device", devName, "error", err, "output", string(out))
	return err
}

// drvRegex matches MakeMKV v2.0.0 DRV lines: DRV:index,visible,enabled,flags,"drive name","disc name","device"
var drvRegex = regexp.MustCompile(`^DRV:(\d+),(\d+),(\d+),(\d+),"([^"]+)","([^"]+)","([^"]*)"`)

// tinfoRegex matches title metadata: TINFO:titleIndex,code,flags,"value"
var tinfoRegex = regexp.MustCompile(`^TINFO:(\d+),(\d+),\d+,"(.*)"`)

// parseDiscInfo parses makemkvcon info output to extract disc info.
// MakeMKV v2.0.0 changed output format: DRV lines now contain disc name.
// Only the v2 robot-mode DRV format is supported; an older TITLE:/STREO:
// fallback once lived here but never matched real output and was removed.
//
// The disc hash is SHA-256 over the sorted (titleIndex, code, value) TINFO
// triples, so identically-named discs with different title structure hash
// differently while the same disc hashes stably across runs.
func parseDiscInfo(output string) DiscInfo {
	lines := strings.Split(output, "\n")

	var title string
	type titleMeta struct {
		pairs []string
	}
	metas := map[string]*titleMeta{}

	for _, line := range lines {
		// DRV format: DRV:index,visible,enabled,flags,"drive name","disc name","/dev/sr0"
		// matches[5]=drive name, matches[6]=disc name, matches[7]=device
		if matches := drvRegex.FindStringSubmatch(line); matches != nil {
			driveName := matches[5]
			discName := matches[6]
			if discName != "" {
				logger.Debug("parseDiscInfo DRV", "drive", driveName, "disc", discName)
			}
			if title == "" && discName != "" && discName != "DVDVolume" && discName != "Blu-ray Disc" {
				title = discName
				logger.Info("parseDiscInfo using disc name", "title", title)
			}
			continue
		}
		// TINFO:2,28,0,"0" — collect every (code,value) pair per title.
		if matches := tinfoRegex.FindStringSubmatch(line); matches != nil {
			idx, code := matches[1], matches[2]
			meta, ok := metas[idx]
			if !ok {
				meta = &titleMeta{}
				metas[idx] = meta
			}
			meta.pairs = append(meta.pairs, code+"="+matches[3])
		}
	}

	var hash string
	if len(metas) > 0 {
		ids := make([]string, 0, len(metas))
		for id := range metas {
			ids = append(ids, id)
		}
		sort.Strings(ids)
		var b strings.Builder
		for _, id := range ids {
			meta := metas[id]
			sort.Strings(meta.pairs)
			b.WriteString(fmt.Sprintf("T%s;%s\n", id, strings.Join(meta.pairs, ";")))
		}
		sum := sha256.Sum256([]byte(b.String()))
		hash = hex.EncodeToString(sum[:])
	}

	return DiscInfo{
		Title: title,
		Hash:  hash,
	}
}

// titleAddedRegex matches MSG:3028 title-added lines.
// Example: MSG:3028,0,3,"Title #1 was added (18 cell(s), 2:10:16)",...
var titleAddedRegex = regexp.MustCompile(`MSG:3028,\d+,\d+,"Title #(\d+) was added`)

// ripStartedRegex matches MSG:5014 saving line.
// Example: MSG:5014,131072,2,"Saving 1 titles into directory file:///tmp/mkv_rip",...
var ripStartedRegex = regexp.MustCompile(`MSG:5014,\d+,\d+,"Saving (\d+) titles`)

// ripCompleteRegex matches MSG:5011 completion line.
var ripCompleteRegex = regexp.MustCompile(`MSG:5011,\d+,\d+,"Operation successfully completed"`)

// prgtRegex matches phase transitions: PRGT:code,0,"phase name"
var prgtRegex = regexp.MustCompile(`^PRGT:\d+,\d+,"([^"]*)"`)

// saveResultRegex matches MSG:5004 "X titles saved, Y failed"
var saveResultRegex = regexp.MustCompile(`MSG:5004,\d+,\d+,"(\d+) titles saved, (\d+) failed"`)

// prgcRegex matches per-title progress: PRGC:id,current,"Title #N ..."
var prgcRegex = regexp.MustCompile(`^PRGC:\d+,\d+,"([^"]*)"`)

// titleNumberRegex extracts title number from PRGC name.
var titleNumberRegex = regexp.MustCompile(`Title #(\d+)`)

// prgvRegex matches MakeMKV progress lines: PRGV:current,total,max
var prgvRegex = regexp.MustCompile(`^PRGV:(\d+),(\d+),(\d+)$`)

// msg5017Regex captures MSG:5017, the per-title "title N saved" message.
// usage.txt documents only the generic MSG line shape
// (MSG:code,flags,count,"message",format,param0,param1,...), so the title
// index/total are read from the trailing numeric params (locale-independent)
// and fall back to the "N of M" pair inside the localized message text.
var msg5017Regex = regexp.MustCompile(`^MSG:5017,\d+,\d+,"((?:[^"\\]|\\.)*)"(.*)$`)

// savedOfRegex extracts "N of M" from a title-saved message text.
var savedOfRegex = regexp.MustCompile(`(\d+)\D{1,10}(\d+)`)

// robotTrailingParams returns the unquoted param fields of a MSG line given
// the remainder after the message field's closing quote: it skips the
// (quoted) format field, then splits on commas and drops quoted values —
// only bare tokens (numbers in practice) are returned.
func robotTrailingParams(rest string) []string {
	// Skip separator + format field.
	rest = strings.TrimPrefix(rest, ",")
	if strings.HasPrefix(rest, `"`) {
		// Scan past the escaped-quote-aware format string.
		i := 1
		for i < len(rest) {
			if rest[i] == '\\' {
				i += 2
				continue
			}
			if rest[i] == '"' {
				break
			}
			i++
		}
		if i >= len(rest) {
			return nil
		}
		rest = strings.TrimPrefix(rest[i+1:], ",")
	}
	var out []string
	for _, f := range strings.Split(rest, ",") {
		if f != "" && !strings.HasPrefix(f, `"`) {
			out = append(out, f)
		}
	}
	return out
}

func parseProgress(line string) *RipProgress {
	// PRGT: phase transition
	if matches := prgtRegex.FindStringSubmatch(line); matches != nil {
		return &RipProgress{Status: "phase", Phase: matches[1]}
	}

	// PRGC: sub-phase label, or "Title #N" discovery
	if matches := prgcRegex.FindStringSubmatch(line); matches != nil {
		name := matches[1]
		if tn := titleNumberRegex.FindStringSubmatch(name); tn != nil {
			n, _ := strconv.Atoi(tn[1])
			return &RipProgress{Status: "title", Title: n}
		}
		return &RipProgress{Status: "subphase", Phase: name}
	}

	// MSG:5004 final save count
	if matches := saveResultRegex.FindStringSubmatch(line); matches != nil {
		saved, _ := strconv.Atoi(matches[1])
		failed, _ := strconv.Atoi(matches[2])
		return &RipProgress{Status: "save_result", Saved: saved, Failed: failed,
			Message: fmt.Sprintf("%d titles saved, %d failed", saved, failed)}
	}

	// MSG:5017 per-title saved: index/total from the trailing numeric params,
	// falling back to an "N of M" pair inside the localized message text.
	// N3 — drives the "Saving title N of M" label in the UI.
	if m := msg5017Regex.FindStringSubmatch(line); m != nil {
		idx, total := 0, 0
		for _, tok := range robotTrailingParams(m[2]) {
			n, err := strconv.Atoi(tok)
			if err != nil || n <= 0 {
				continue
			}
			if idx == 0 {
				idx = n
			} else if total == 0 {
				total = n
			}
		}
		if idx == 0 || total == 0 {
			if p := savedOfRegex.FindStringSubmatch(m[1]); p != nil {
				a, _ := strconv.Atoi(p[1])
				b, _ := strconv.Atoi(p[2])
				if idx == 0 {
					idx = a
				}
				if total == 0 {
					total = b
				}
			}
		}
		if total < idx {
			total = idx
		}
		return &RipProgress{
			Status:     "title_saved",
			TitleIndex: idx,
			TitleTotal: total,
			Message:    fmt.Sprintf("Saving title %d of %d", idx, total),
		}
	}

	// MSG:3028 title-added messages
	if matches := titleAddedRegex.FindStringSubmatch(line); matches != nil {
		titleNum, _ := strconv.Atoi(matches[1])
		return &RipProgress{
			Title:   titleNum,
			Status:  "title_added",
			Message: fmt.Sprintf("Title #%d discovered", titleNum),
		}
	}

	// MSG:5014 saving started
	if matches := ripStartedRegex.FindStringSubmatch(line); matches != nil {
		numTitles, _ := strconv.Atoi(matches[1])
		return &RipProgress{
			Title:   numTitles,
			Status:  "saving",
			Message: fmt.Sprintf("Ripping %d title(s)", numTitles),
		}
	}

	// MSG:5011 completion (log only; never triggers completion — file writes may still be in progress)
	if ripCompleteRegex.MatchString(line) {
		return &RipProgress{Status: "makemkv_complete"}
	}

	// PRGV: progress line — current,total,max
	if matches := prgvRegex.FindStringSubmatch(line); matches != nil {
		cur, _ := strconv.Atoi(matches[1])
		max, _ := strconv.Atoi(matches[3])
		if max == 0 {
			return nil
		}
		pct := 100.0 * float64(cur) / float64(max)
		return &RipProgress{Status: "prgv", Percent: pct, Current: cur, Max: max}
	}

	return nil
}

// mkvStats counts .mkv files and total bytes in a directory.
func mkvStats(dir string) (count int, sizeBytes int64) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return 0, 0
	}
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		if strings.HasSuffix(strings.ToLower(e.Name()), ".mkv") {
			count++
			info, err := e.Info()
			if err == nil {
				sizeBytes += info.Size()
			}
		}
	}
	return count, sizeBytes
}

// mediaCapacityBytes reads the kernel-observed media capacity from sysfs.
// /sys/block/<name>/size is in 512-byte sectors. Returns 0 when unknown.
func mediaCapacityBytes(devPath string) int64 {
	name := strings.TrimPrefix(devPath, "/dev/")
	data, err := os.ReadFile("/sys/block/" + name + "/size")
	if err != nil {
		return 0
	}
	sectors, err := strconv.ParseInt(strings.TrimSpace(string(data)), 10, 64)
	if err != nil || sectors <= 0 {
		return 0
	}
	return sectors * 512
}
// extractVersion parses the MakeMKV version from the MSG:1005 output line.
// Example: MSG:1005,0,1,"MakeMKV v2.0.0 linux(x64-release) started",...
func extractVersion(output string) string {
	for _, line := range strings.Split(output, "\n") {
		if strings.Contains(line, `MSG:1005`) {
			re := regexp.MustCompile(`MakeMKV\s+(v[\d.]+)`)
			if m := re.FindStringSubmatch(line); m != nil {
				return m[1]
			}
		}
	}
	return "unknown"
}

// DriveIndex derives the MakeMKV drive index from a device path like /dev/sr12.
// It parses the numeric suffix; /dev/sr0 → 0, /dev/sr1 → 1, /dev/sr12 → 12.
// Paths that do not match /dev/srN return 0 and log a warning.
func DriveIndex(devPath string) int {
	const prefix = "/dev/sr"
	if !strings.HasPrefix(devPath, prefix) {
		logger.Warn("DriveIndex: unrecognized device path", "path", devPath)
		return 0
	}
	suffix := devPath[len(prefix):]
	idx, err := strconv.Atoi(suffix)
	if err != nil {
		logger.Warn("DriveIndex: non-numeric suffix", "path", devPath)
		return 0
	}
	return idx
}

// errMSGTextRegex extracts the message text field of a robot-mode MSG line.
var errMSGTextRegex = regexp.MustCompile(`^MSG:\d+,\d+,\d+,"((?:[^"\\]|\\.)*)"`)

// makemkvErrorHint picks the most recent human-readable MSG text from the raw
// output tail so a failed rip reports MakeMKV's own reason (e.g. "Failed to
// open disc", "The program can't find any usable optical drives.") instead of
// the opaque bare "exit status 11". Scans backwards because the terminal
// error is emitted last; returns "" when the tail holds no usable text.
func makemkvErrorHint(lines []string) string {
	for i := len(lines) - 1; i >= 0; i-- {
		m := errMSGTextRegex.FindStringSubmatch(lines[i])
		if m == nil {
			continue
		}
		text := strings.TrimSpace(strings.ReplaceAll(m[1], `\"`, `"`))
		if text == "" || strings.Contains(text, "started") {
			continue // banner and empty messages are not failure reasons
		}
		return truncate(text, 200)
	}
	return ""
}
