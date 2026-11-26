package rip

import (
	"bytes"
	"context"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
	"time"
)

func TestParseDiscInfo(t *testing.T) {
	// MASK disc name should be extracted from DRV line
	input := `DRV:0,2,999,1,"DVD+R-DL TS8XDVDR Transcend 1.67 R8CK6GBBB12538","MASK","/dev/sr0"
DRV:1,256,999,0,"","",""`
	info := parseDiscInfo(input)
	if info.Title != "MASK" {
		t.Errorf("parseDiscInfo(MASK) = %q, want %q", info.Title, "MASK")
	}

	// Empty disc name should not fall back to drive name
	input2 := `DRV:0,2,999,1,"Some Drive Model","","/dev/sr0"`
	info2 := parseDiscInfo(input2)
	if info2.Title != "" {
		t.Errorf("parseDiscInfo(empty disc) = %q, want %q", info2.Title, "")
	}

	// Generic placeholders should be skipped
	input3 := `DRV:0,2,999,1,"Drive","DVDVolume","/dev/sr0"`
	info3 := parseDiscInfo(input3)
	if info3.Title != "" {
		t.Errorf("parseDiscInfo(DVDVolume) = %q, want %q", info3.Title, "")
	}

	input4 := `DRV:0,2,999,1,"Drive","Blu-ray Disc","/dev/sr0"`
	info4 := parseDiscInfo(input4)
	if info4.Title != "" {
		t.Errorf("parseDiscInfo(Blu-ray Disc) = %q, want %q", info4.Title, "")
	}

	input5 := `DRV:0,2,999,1,"Drive","MASK","/dev/sr0"
TINFO:0,8,0,"27"
TINFO:0,9,0,"1:36:47"
TINFO:0,11,0,"4539170816"
TINFO:1,9,0,"0:27:15"
TINFO:1,11,0,"853141504"`
	info5 := parseDiscInfo(input5)
	if info5.Title != "MASK" {
		t.Errorf("parseDiscInfo(MASK) title = %q, want MASK", info5.Title)
	}
}

func TestHasMediaInOutput(t *testing.T) {
	// Media present
	if !hasMediaInOutput(`DRV:0,2,999,1,"Drive","MASK","/dev/sr0"`) {
		t.Error("hasMediaInOutput(MASK) = false, want true")
	}

	// Non-zero media flags (bitmask) should also return true
	if !hasMediaInOutput(`DRV:0,2,999,3,"Drive","MASK","/dev/sr0"`) {
		t.Error("hasMediaInOutput(flags=3) = false, want true")
	}

	// Zero flags means no media
	if hasMediaInOutput(`DRV:0,256,999,0,"","",""`) {
		t.Error("hasMediaInOutput(flags=0) = true, want false")
	}
}

func TestParseProgress(t *testing.T) {
	tests := []struct {
		name       string
		input      string
		wantStatus string
		wantTitle  int
		wantPhase  string
		wantSaved  int
		wantFailed int
		wantNil    bool
	}{
		{"PRGT phase", `PRGT:3120,0,"Scanning contents"`, "phase", 0, "Scanning contents", 0, 0, false},
		{"PRGC title", `PRGC:5036,0,"Title #1"`, "title", 1, "", 0, 0, false},
		{"PRGC subphase", `PRGC:5017,2,"Saving to MKV file"`, "subphase", 0, "Saving to MKV file", 0, 0, false},
		{"title added", `MSG:3028,0,3,"Title #5 was added (18 cell(s), 2:10:16)",...`, "title_added", 5, "", 0, 0, false},
		{"saving", `MSG:5014,131072,2,"Saving 5 titles into directory file:///tmp/x",...`, "saving", 5, "", 0, 0, false},
		{"save result", `MSG:5004,128,2,"4 titles saved, 1 failed"`, "save_result", 0, "", 4, 1, false},
		{"makemkv complete", `MSG:5011,0,0,"Operation successfully completed"`, "makemkv_complete", 0, "", 0, 0, false},
		{"garbage", "garbage", "", 0, "", 0, 0, true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			p := parseProgress(tt.input)
			if tt.wantNil {
				if p != nil {
					t.Errorf("parseProgress(%q) = %+v, want nil", tt.input, p)
				}
				return
			}
			if p == nil {
				t.Errorf("parseProgress(%q) = nil, want status %q", tt.input, tt.wantStatus)
				return
			}
			if p.Status != tt.wantStatus {
				t.Errorf("parseProgress(%q) status = %q, want %q", tt.input, p.Status, tt.wantStatus)
			}
			if p.Title != tt.wantTitle {
				t.Errorf("parseProgress(%q) title = %d, want %d", tt.input, p.Title, tt.wantTitle)
			}
			if p.Phase != tt.wantPhase {
				t.Errorf("parseProgress(%q) phase = %q, want %q", tt.input, p.Phase, tt.wantPhase)
			}
			if p.Saved != tt.wantSaved {
				t.Errorf("parseProgress(%q) saved = %d, want %d", tt.input, p.Saved, tt.wantSaved)
			}
			if p.Failed != tt.wantFailed {
				t.Errorf("parseProgress(%q) failed = %d, want %d", tt.input, p.Failed, tt.wantFailed)
			}
		})
	}
}

// N3: MSG:5017 per-title saved, per the robot line shape in
// makemkv.com/developers/usage.txt (MSG:code,flags,count,"message",format,
// param0,param1,...). Index/total come from the trailing numeric params;
// when MakeMKV omits them the "N of M" pair in the message text is used.
func TestParseProgressTitleSaved(t *testing.T) {
	tests := []struct {
		name       string
		input      string
		wantIdx    int
		wantTotal  int
		wantStatus string
	}{
		{
			"numeric params",
			`MSG:5017,131072,2,"Title 2 saved","Title %1 of %2 saved",2,5,"file:///tmp/out/title_t01.mkv"`,
			2, 5, "title_saved",
		},
		{
			"no numeric params, N of M in text",
			`MSG:5017,131072,1,"Title 3 of 5 saved","Title %1 saved"`,
			3, 5, "title_saved",
		},
		{
			"first title",
			`MSG:5017,131072,2,"Title 1 saved","Title %1 of %2 saved",1,1,"file:///tmp/out/title_t00.mkv"`,
			1, 1, "title_saved",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			p := parseProgress(tt.input)
			if p == nil {
				t.Fatalf("parseProgress(%q) = nil, want status %q", tt.input, tt.wantStatus)
			}
			if p.Status != tt.wantStatus {
				t.Errorf("status = %q, want %q", p.Status, tt.wantStatus)
			}
			if p.TitleIndex != tt.wantIdx {
				t.Errorf("TitleIndex = %d, want %d", p.TitleIndex, tt.wantIdx)
			}
			if p.TitleTotal != tt.wantTotal {
				t.Errorf("TitleTotal = %d, want %d", p.TitleTotal, tt.wantTotal)
			}
		})
	}
}

func TestMkvStats(t *testing.T) {
	dir := t.TempDir()

	// Create two .mkv files with known sizes
	f1 := filepath.Join(dir, "test1.mkv")
	f2 := filepath.Join(dir, "test2.mkv")
	f3 := filepath.Join(dir, "notmkv.txt")

	os.WriteFile(f1, []byte{0, 1, 2, 3, 4}, 0644)     // 5 bytes
	os.WriteFile(f2, []byte{5, 6, 7, 8, 9, 10}, 0644) // 6 bytes
	os.WriteFile(f3, []byte("hello"), 0644)           // 5 bytes (ignored)

	count, bytes := mkvStats(dir)
	if count != 2 {
		t.Errorf("mkvStats count = %d, want 2", count)
	}
	if bytes != 11 {
		t.Errorf("mkvStats bytes = %d, want 11", bytes)
	}
}

func writeFakeMakemkv(t *testing.T, body string) string {
	t.Helper()
	script := filepath.Join(t.TempDir(), "fake-makemkvcon")
	// L4: Rip/GetDiscInfo now resolve the drive index via an enumeration run
	// (`-r info disc:9999`) before the real command; answer it with a
	// single DRV entry so the fake does not re-run the test body.
	full := "if [ \"$3\" = \"disc:9999\" ]; then echo 'DRV:0,2,999,1,\"Fake Drive\",\"X\",\"/dev/sr0\"'; exit 0; fi\n" + body + "\n"
	if err := os.WriteFile(script, []byte("#!/bin/sh\n"+full), 0o755); err != nil {
		t.Fatal(err)
	}
	return script
}

func TestRipPipeline(t *testing.T) {
	// $6 is the output directory: args are
	// --progress=-stdout -r mkv disc:0 all <outDir>
	script := writeFakeMakemkv(t, `out="$6"
echo 'PRGT:5024,0,"Saving all titles to MKV files"'
echo 'MSG:5014,0,2,"Saving 2 titles into directory x"'
head -c 500 /dev/zero > "$out/title_t00.mkv"
echo 'PRGV:32768,65536,65536'
sleep 1.3
head -c 500 /dev/zero > "$out/title_t01.mkv"
echo 'MSG:5004,0,2,"2 titles saved, 0 failed"'`)
	m := NewMakeMKV(script)
	outDir := filepath.Join(t.TempDir(), "out")
	var last RipProgress
	sawMid, sawPhase := false, false
	for p := range m.Rip(context.Background(), "/dev/sr99", outDir, 0) {
		last = p
		if p.Status == "ripping" && p.Percent > 20 && p.Percent < 80 {
			sawMid = true
		}
		if p.Phase == "Saving all titles to MKV files" {
			sawPhase = true
		}
	}
	if last.Status != "complete" {
		t.Fatalf("last status = %q, want complete", last.Status)
	}
	if last.Percent != 100 {
		t.Errorf("final percent = %v, want 100", last.Percent)
	}
	if !sawMid {
		t.Error("expected intermediate percent between 20 and 80")
	}
	if !sawPhase {
		t.Error("expected phase label on progress events")
	}
}

// The consumer must tag early progress events with RipPhase "scanning" and
// switch to "saving" once MSG:5014 arrives; the final event is "complete".
func TestRipPipelineRipPhase(t *testing.T) {
	script := writeFakeMakemkv(t, `out="$6"
echo 'PRGT:3120,0,"Scanning contents"'
sleep 0.3
echo 'MSG:5014,0,2,"Saving 2 titles into directory x"'
head -c 500 /dev/zero > "$out/title_t00.mkv"
sleep 0.3
head -c 500 /dev/zero > "$out/title_t01.mkv"
echo 'MSG:5004,0,2,"2 titles saved, 0 failed"'
`)
	m := NewMakeMKV(script)
	outDir := filepath.Join(t.TempDir(), "out")
	var sawScanning, sawSaving bool
	for p := range m.Rip(context.Background(), "/dev/sr0", outDir, 0) {
		if p.Status == "ripping" && p.RipPhase == "scanning" {
			sawScanning = true
		}
		if p.Status == "ripping" && p.RipPhase == "saving" {
			sawSaving = true
		}
		if p.Status == "complete" && p.RipPhase != "complete" {
			t.Errorf("complete event RipPhase = %q, want complete", p.RipPhase)
		}
	}
	if !sawScanning {
		t.Error("expected at least one ripping event with RipPhase=scanning")
	}
	if !sawSaving {
		t.Error("expected at least one ripping event with RipPhase=saving")
	}
}

func TestRipPipelineAllFailed(t *testing.T) {
	script := writeFakeMakemkv(t, `echo 'MSG:5004,0,5,"0 titles saved, 5 failed"'`)
	m := NewMakeMKV(script)
	var last RipProgress
	for p := range m.Rip(context.Background(), "/dev/sr0", filepath.Join(t.TempDir(), "out"), 0) {
		last = p
	}
	if last.Status != "error" {
		t.Fatalf("last status = %q, want error", last.Status)
	}
}

func TestRipPipelineZeroFiles(t *testing.T) {
	// makemkvcon exits 0, reports a save phase, but writes no .mkv files
	// and never emits MSG:5004.
	script := writeFakeMakemkv(t, `echo 'MSG:5014,0,2,"Saving 1 titles into directory x"'`)
	m := NewMakeMKV(script)
	var last RipProgress
	for p := range m.Rip(context.Background(), "/dev/sr0", filepath.Join(t.TempDir(), "out"), 0) {
		last = p
	}
	if last.Status != "error" {
		t.Fatalf("last status = %q, want error", last.Status)
	}
	if !strings.Contains(last.Message, "No files produced") {
		t.Errorf("message = %q, want mention of no files", last.Message)
	}
}

func TestRipPipelineZeroFilesZeroFailed(t *testing.T) {
	// Explicit "0 titles saved, 0 failed" with no files written.
	script := writeFakeMakemkv(t, `echo 'MSG:5004,0,2,"0 titles saved, 0 failed"'`)
	m := NewMakeMKV(script)
	var last RipProgress
	for p := range m.Rip(context.Background(), "/dev/sr0", filepath.Join(t.TempDir(), "out"), 0) {
		last = p
	}
	if last.Status != "error" {
		t.Fatalf("last status = %q, want error", last.Status)
	}
}

// A makemkvcon failure must surface the tool's own terminal message, not just
// the bare exit code — the USB-drop failures were reported as opaque
// "makemkvcon: exit status 11".
func TestRipErrorIncludesMakemkvMessage(t *testing.T) {
	script := writeFakeMakemkv(t, `echo 'MSG:1005,0,1,"MakeMKV v2.0.0 linux(x64-release) started"'
echo 'PRGT:5018,0,"Scanning CD-ROM devices"'
echo 'MSG:5010,0,0,"Failed to open disc"'
exit 11`)
	m := NewMakeMKV(script)
	var last RipProgress
	for p := range m.Rip(context.Background(), "/dev/sr0", filepath.Join(t.TempDir(), "out"), 0) {
		last = p
	}
	if last.Status != "error" {
		t.Fatalf("last status = %q, want error", last.Status)
	}
	if !strings.Contains(last.Message, "exit status 11") {
		t.Errorf("message = %q, want exit status", last.Message)
	}
	if !strings.Contains(last.Message, "Failed to open disc") {
		t.Errorf("message = %q, want MakeMKV's own reason appended", last.Message)
	}
}

func TestRawCapture(t *testing.T) {
	capturePath := filepath.Join(t.TempDir(), "raw.log")
	t.Setenv("RIPLY_MAKEMKV_LOG", capturePath)

	script := writeFakeMakemkv(t, `echo 'DRV:0,2,999,1,"Drive","MASK","/dev/sr0"'
echo 'PRGV:0,100,100'
echo 'MSG:5014,0,1,"Saving 1 titles into directory x"'
head -c 100 /dev/zero > "$6/title_t00.mkv"
echo 'MSG:5004,1,0,"1 titles saved, 0 failed"'`)

	m := NewMakeMKV(script)
	outDir := filepath.Join(t.TempDir(), "out")
	for p := range m.Rip(context.Background(), "/dev/sr0", outDir, 0) {
		if p.Status == "complete" {
			break
		}
	}

	data, err := os.ReadFile(capturePath)
	if err != nil {
		t.Fatalf("read capture file: %v", err)
	}
	content := string(data)
	if !strings.Contains(content, `DRV:0,2,999,1,"Drive","MASK","/dev/sr0"`) {
		t.Error("capture file missing DRV line")
	}
	if !strings.Contains(content, `PRGV:0,100,100`) {
		t.Error("capture file missing PRGV line")
	}
	if !strings.Contains(content, `MSG:5014,0,1,"Saving 1 titles into directory x"`) {
		t.Error("capture file missing MSG:5014 line")
	}
}

func TestRipPipelinePRGV(t *testing.T) {
	// Fake makemkvcon script using captured PRGV format (PRGV:current,total,max with max=65536)
	script := writeFakeMakemkv(t, `echo 'DRV:0,2,999,1,"Drive","MASK","/dev/sr0"'
echo 'PRGV:0,65536,65536'
echo 'MSG:5014,0,2,"Saving 2 titles into directory x"'
sleep 0.2
head -c 100 /dev/zero > "$6/title_t00.mkv"
echo 'PRGV:8192,65536,65536'
sleep 0.2
echo 'PRGV:16384,65536,65536'
head -c 100 /dev/zero > "$6/title_t01.mkv"
echo 'PRGV:32768,65536,65536'
echo 'PRGV:65536,65536,65536'
echo 'MSG:5004,2,0,"2 titles saved, 0 failed"'`)

	m := NewMakeMKV(script)
	outDir := filepath.Join(t.TempDir(), "out")
	var sawMid, sawComplete bool
	var last RipProgress
	for p := range m.Rip(context.Background(), "/dev/sr0", outDir, 0) {
		last = p
		if p.Status == "complete" {
			sawComplete = true
			break
		}
		if p.Status == "ripping" && p.Percent > 0 && p.Percent < 100 {
			sawMid = true
		}
	}
	if !sawMid {
		t.Error("expected intermediate progress event with 0 < Percent < 100")
	}
	if !sawComplete {
		t.Fatal("expected final complete event")
	}
	if last.Status != "complete" {
		t.Fatalf("last status = %q, want complete", last.Status)
	}
	if last.Percent != 100 {
		t.Errorf("final percent = %v, want 100", last.Percent)
	}
}

// M9: a robot line over the 1 MiB buffer limit must not end the reader loop.
// scanRobotLines drops the oversized line (with one log) and keeps parsing;
// the old bufio.Scanner silently stopped consuming, leaving the parse loop
// waiting on a half-drained pipe until the process exited.
func TestScanRobotLinesOversizedLineDrains(t *testing.T) {
	long := strings.Repeat("x", 2<<20) // 2 MiB > 1 MiB limit
	input := "PRGV:0,32768,65536\n" + long + "\nMSG:5014,0,2,\"Saving 2 titles\"\n"
	out := make(chan string, 8)
	done := make(chan error, 1)
	go func() { done <- scanRobotLines(strings.NewReader(input), out) }()

	// scanRobotLines returns when the reader is drained but does not close
	// out (the callers do), so stop on done, not channel close.
	var got []string
	timeout := time.After(5 * time.Second)
	for {
		select {
		case l := <-out:
			got = append(got, l)
		case err := <-done:
			for {
				select {
				case l := <-out:
					got = append(got, l)
				default:
					if err != nil {
						t.Fatalf("scanRobotLines error = %v", err)
					}
					if len(got) != 2 {
						t.Fatalf("forwarded lines = %d (%q...), want 2 (oversized line dropped)", len(got), got)
					}
					if !strings.HasPrefix(got[0], "PRGV:") || !strings.HasPrefix(got[1], "MSG:") {
						t.Errorf("forwarded lines = %q", got)
					}
					return
				}
			}
		case <-timeout:
			t.Fatal("scanRobotLines did not finish within 5s: stuck or wedge")
		}
	}
}

// M9 end-to-end: fake makemkvcon prints a >1MiB line mid-rip then normal
// output; the pipeline must still deliver completion (old behavior: parse
// loop stops consuming while the process runs).
func TestRipPipelineSurvivesOversizedLine(t *testing.T) {
	script := writeFakeMakemkv(t, `out="$6"
echo 'MSG:5014,0,2,"Saving 2 titles into directory x"'
head -c 500 /dev/zero > "$out/title_t00.mkv"
echo 'PRGV:0,32768,65536'
# oversized garbage line (>1 MiB)
awk 'BEGIN{for(i=0;i<2200;i++)printf "%01023d", i}'
echo ''
head -c 500 /dev/zero > "$out/title_t01.mkv"
echo 'MSG:5004,0,2,"2 titles saved, 0 failed"'
`)
	m := NewMakeMKV(script)
	outDir := filepath.Join(t.TempDir(), "out")
	var last RipProgress
	sawAfterGarbage := false
	for p := range m.Rip(context.Background(), "/dev/sr0", outDir, 0) {
		last = p
		if p.Status == "ripping" {
			sawAfterGarbage = true
		}
	}
	if last.Status != "complete" {
		t.Fatalf("last status = %q, want complete (garbage line must not wedge the parser)", last.Status)
	}
	if !sawAfterGarbage {
		t.Error("no ripping progress seen; parse loop likely stopped at the oversized line")
	}
}

// L3: a hanging makemkvcon (timeout, no version banner) must be reported as
// unavailable — the old code treated any timeout as success.
func TestProbeTimeoutUnavailable(t *testing.T) {
	origTO := probeTimeout
	t.Cleanup(func() { probeTimeout = origTO })
	probeTimeout = 300 * time.Millisecond

	script := writeFakeMakemkv(t, `echo 'PRGV:0,1,1'; exec sleep 10`)
	m := NewMakeMKV(script)
	if m.IsAvailable() {
		t.Fatal("hanging makemkvcon reported available")
	}
}

// L3: timeout AFTER the version banner (MSG:1005) printed still means the
// binary exists — available.
func TestProbeTimeoutAfterBannerAvailable(t *testing.T) {
	origTO := probeTimeout
	t.Cleanup(func() { probeTimeout = origTO })
	probeTimeout = 300 * time.Millisecond

	script := writeFakeMakemkv(t, `echo 'MSG:1005,0,0,"MakeMKV v2.0.0 by www.makemkv.com"'
echo 'MSG:5010,0,0,"Opening disc"'
exec sleep 10`)
	m := NewMakeMKV(script)
	if !m.IsAvailable() {
		t.Fatal("makemkvcon that printed the banner should be available despite timeout")
	}
	if v := m.Version(); v != "v2.0.0" {
		t.Errorf("version = %q, want v2.0.0", v)
	}
}

// L3: MakeMKV missing at startup then installed later — IsAvailable must
// re-probe once the stale negative verdict ages out, without a restart.
func TestIsAvailableReprobesAfterMiss(t *testing.T) {
	origTO, origTTL, origRetry := probeTimeout, availabilityTTL, probeRetryInterval
	t.Cleanup(func() { probeTimeout, availabilityTTL, probeRetryInterval = origTO, origTTL, origRetry })
	probeTimeout = time.Second
	availabilityTTL = 50 * time.Millisecond
	probeRetryInterval = 10 * time.Millisecond

	dir := t.TempDir()
	script := filepath.Join(dir, "fake-makemkvcon")

	// 1. No binary yet → unavailable.
	m := NewMakeMKV(script)
	if m.IsAvailable() {
		t.Fatal("missing makemkvcon reported available")
	}

	// 2. Install it; after the retry interval IsAvailable must see it.
	if err := os.WriteFile(script, []byte("#!/bin/sh\necho 'MSG:1005,0,0,\"MakeMKV v2.0.0 by www.makemkv.com\"'\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	deadline := time.Now().Add(3 * time.Second)
	for time.Now().Before(deadline) {
		if m.IsAvailable() {
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatal("IsAvailable never recovered after makemkvcon was installed")
}

// L4: MakeMKV's drive index must come from the DRV enumeration, not the
// /dev/srN suffix. Here makemkvcon enumerates /dev/sr5 as index 1, so a rip
// of /dev/sr5 must run with disc:1 (the /dev/srN heuristic would use disc:5).
func TestRipTargetsDRVEntryMatchingDevicePath(t *testing.T) {
	dir := t.TempDir()
	record := filepath.Join(dir, "args.txt")
	script := filepath.Join(dir, "fake-makemkvcon")
	body := `#!/bin/sh
if [ "$3" = "disc:9999" ]; then
  echo 'DRV:0,256,999,0,"","",""'
  echo 'DRV:1,2,999,1,"USB Writer","MASK","/dev/sr5"'
  exit 0
fi
if [ "$5" = "all" ]; then
  echo "rip $4" >> ` + record + `
  head -c 128 /dev/zero > "$6/title_t00.mkv"
  echo 'MSG:5004,0,2,"1 titles saved, 0 failed"'
  exit 0
fi
echo "info $5" >> ` + record + `
echo 'DRV:1,2,999,1,"USB Writer","MASK","/dev/sr5"'
exit 0
`
	if err := os.WriteFile(script, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	m := NewMakeMKV(script)

	outDir := filepath.Join(dir, "out")
	for p := range m.Rip(context.Background(), "/dev/sr5", outDir, 0) {
		if p.Status == "complete" {
			break
		}
	}

	data, err := os.ReadFile(record)
	if err != nil {
		t.Fatalf("rip never executed: %v", err)
	}
	s := string(data)
	if !strings.Contains(s, "rip disc:1") {
		t.Errorf("rip used wrong disc arg (want disc:1 for /dev/sr5): %q", s)
	}
	if strings.Contains(s, "disc:5") {
		t.Errorf("rip fell back to /dev/srN suffix: %q", s)
	}
}

// L4 fallback: when enumeration does not list the device, /dev/srN suffix is
// used (preserves the old behavior instead of failing the rip outright).
func TestRipFallsBackWhenDeviceNotEnumerated(t *testing.T) {
	script := filepath.Join(t.TempDir(), "fake-makemkvcon")
	record := filepath.Join(t.TempDir(), "args.txt")
	body := `#!/bin/sh
if [ "$3" = "disc:9999" ]; then
  echo 'DRV:0,256,999,0,"","",""'
  exit 0
fi
if [ "$5" = "all" ]; then
  echo "rip $4" >> ` + record + `
  head -c 128 /dev/zero > "$6/title_t00.mkv"
  exit 0
fi
exit 0
`
	if err := os.WriteFile(script, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	m := NewMakeMKV(script)
	for range m.Rip(context.Background(), "/dev/sr3", filepath.Join(t.TempDir(), "out"), 0) {
	}
	data, err := os.ReadFile(record)
	if err != nil {
		t.Fatalf("rip never executed: %v", err)
	}
	if !strings.Contains(string(data), "rip disc:3") {
		t.Errorf("fallback to /dev/srN suffix failed: %q", data)
	}
}

// N2: SetMinTitleLength(n) must add --minlength=n to the mkv argv; a value
// of 0 (the ripper default) must omit the flag entirely so MakeMKV rips
// every title.
func TestRipPassesMinLengthFlag(t *testing.T) {
	dir := t.TempDir()
	record := filepath.Join(dir, "argv.txt")
	script := filepath.Join(dir, "fake-makemkvcon")
	body := `#!/bin/sh
if [ "$3" = "disc:9999" ]; then
  echo 'DRV:0,2,999,1,"Fake","X","/dev/sr0"'
  exit 0
fi
# rip invocation: record the full argv, then emit completion
echo "$@" >> ` + record + `
echo 'MSG:5004,0,2,"1 titles saved, 0 failed"'
exit 0
`
	if err := os.WriteFile(script, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}

	// 1. With min length set, the flag appears.
	m := NewMakeMKV(script)
	setter, ok := interface{}(m).(MinLengthSetter)
	if !ok {
		t.Fatal("makeMKV does not implement MinLengthSetter")
	}
	setter.SetMinTitleLength(120)
	for p := range m.Rip(context.Background(), "/dev/sr0", filepath.Join(dir, "out1"), 0) {
		if p.Status == "complete" {
			break
		}
	}
	data, err := os.ReadFile(record)
	if err != nil {
		t.Fatalf("rip never executed: %v", err)
	}
	if !strings.Contains(string(data), "--minlength=120") {
		t.Errorf("--minlength=120 missing from argv: %q", data)
	}

	// 2. With min length 0, the flag is omitted.
	os.Remove(record)
	setter.SetMinTitleLength(0)
	for p := range m.Rip(context.Background(), "/dev/sr0", filepath.Join(dir, "out2"), 0) {
		if p.Status == "complete" {
			break
		}
	}
	data, err = os.ReadFile(record)
	if err != nil {
		t.Fatalf("second rip never executed: %v", err)
	}
	if strings.Contains(string(data), "--minlength") {
		t.Errorf("--minlength present with 0 setting: %q", data)
	}
}

// N3: MSG:5017 lines must surface the per-title index/total on the
// "ripping" progress the Rip loop emits (and thus on the rip_progress SSE
// event), while completion stays exit-code + file-count based.
func TestRipSurfacesPerTitleIndexTotal(t *testing.T) {
	script := writeFakeMakemkv(t, `echo 'MSG:5014,131072,2,"Saving 2 titles into directory x"'
echo 'MSG:5017,131072,2,"Title 1 saved","Title %1 of %2 saved",1,2'
head -c 100 /dev/zero > "$6/title_t00.mkv"
echo 'MSG:5017,131072,2,"Title 2 saved","Title %1 of %2 saved",2,2'
head -c 100 /dev/zero > "$6/title_t01.mkv"
echo 'MSG:5004,0,2,"2 titles saved, 0 failed"'
echo 'MSG:5011,0,0,"Operation successfully completed"'
`)
	m := NewMakeMKV(script)
	outDir := filepath.Join(t.TempDir(), "out")

	var sawTitle1, sawTitle2, completed bool
	maxIdx := 0
	for p := range m.Rip(context.Background(), "/dev/sr0", outDir, 0) {
		if p.Status == "complete" {
			completed = true
			break
		}
		if p.Status == "ripping" && p.TitleTotal == 2 {
			if p.TitleIndex > maxIdx {
				maxIdx = p.TitleIndex
			}
			if p.TitleIndex == 1 {
				sawTitle1 = true
			}
			if p.TitleIndex == 2 {
				sawTitle2 = true
			}
		}
	}
	if !completed {
		t.Fatal("rip never completed")
	}
	if !sawTitle1 || !sawTitle2 {
		t.Errorf("ripping events never carried title 1/2 (max seen %d): saw1=%v saw2=%v", maxIdx, sawTitle1, sawTitle2)
	}
}

// The progress bar's remaining-time estimate needs the bytes written so far
// on every "ripping" event (TotalBytes), and SpeedMBPS must be a smoothed
// rate that survives zero-rate ticks instead of dropping to 0.
func TestRipProgressCarriesTotalBytes(t *testing.T) {
	script := writeFakeMakemkv(t, `out="$6"
echo 'MSG:5014,0,2,"Saving 2 titles into directory x"'
head -c 1048576 /dev/zero > "$out/title_t00.mkv"
sleep 8
head -c 2097152 /dev/zero > "$out/title_t01.mkv"
sleep 6
echo 'MSG:5004,0,2,"2 titles saved, 0 failed"'
`)
	m := NewMakeMKV(script)
	outDir := filepath.Join(t.TempDir(), "out")
	var sawBytes int64
	var sawSpeed float64
	for p := range m.Rip(context.Background(), "/dev/sr0", outDir, 0) {
		if p.Status == "complete" {
			break
		}
		if p.Status != "ripping" {
			continue
		}
		if p.TotalBytes > sawBytes {
			sawBytes = p.TotalBytes
		}
		if p.SpeedMBPS > sawSpeed {
			sawSpeed = p.SpeedMBPS
		}
	}
	if sawBytes < 2<<20 {
		t.Errorf("max TotalBytes seen = %d, want >= %d (both titles written)", sawBytes, 2<<20)
	}
	if sawSpeed <= 0 {
		t.Error("expected a positive smoothed SpeedMBPS at some point during the rip")
	}
}

// TestRipByteDrivenPercent verifies that when a totalBytes estimate is
// provided, the progress bar starts near 0% and climbs smoothly based on
// bytes written, rather than jumping to ~99% from PRGV immediately.
func TestRipByteDrivenPercent(t *testing.T) {
	// Simulates a realistic rip timeline: PRGV lines arrive during writing,
	// not after. The old code would jump to ~99% on the first PRGV; with
	// byte-driven percent the bar should climb smoothly from 0%.
	script := writeFakeMakemkv(t, `out="$6"
echo 'PRGT:5024,0,"Saving all titles to MKV files"'
echo 'MSG:5014,0,1,"Saving 1 titles into directory x"'
sleep 1.2
head -c 3145728 /dev/zero > "$out/title_t00.mkv"
sleep 1.2
head -c 3145728 /dev/zero >> "$out/title_t00.mkv"
sleep 1.2
head -c 3145728 /dev/zero >> "$out/title_t00.mkv"
echo 'MSG:5004,1,0,"1 title saved, 0 failed"'`)

	m := NewMakeMKV(script)
	outDir := filepath.Join(t.TempDir(), "out")
	// 12 MiB media capacity × 0.75 rip factor = 9 MiB estimated total.
	// The fake writes 9 MiB in three 3 MiB chunks; no PRGV lines, so only
	// the 1s ticker fires emit — giving predictable byte-driven samples.
	const mediaBytes = int64(12 << 20)

	var rippingEvents []RipProgress
	var last RipProgress
	for p := range m.Rip(context.Background(), "/dev/sr0", outDir, mediaBytes) {
		last = p
		if p.Status == "ripping" {
			rippingEvents = append(rippingEvents, p)
		}
		if p.Status == "complete" {
			break
		}
	}

	if len(rippingEvents) == 0 {
		t.Fatal("expected at least one ripping event")
	}

	// First ripping event should have a low percent (bytes just started).
	first := rippingEvents[0]
	if first.Percent >= 5 {
		t.Errorf("first ripping percent = %.1f, want < 5 (bar should start near 0%%)", first.Percent)
	}

	// Percent must be monotonically non-decreasing across ripping events.
	for i := 1; i < len(rippingEvents); i++ {
		if rippingEvents[i].Percent < rippingEvents[i-1].Percent {
			t.Errorf("percent regressed at event %d: %.1f -> %.1f",
				i, rippingEvents[i-1].Percent, rippingEvents[i].Percent)
		}
	}

	// At least one mid-rip event should be in the 5–90 range (bytes accumulating).
	sawMid := false
	for _, p := range rippingEvents {
		if p.Percent >= 5 && p.Percent <= 90 {
			sawMid = true
			break
		}
	}
	if !sawMid {
		t.Errorf("expected a mid-rip event with 5%% <= Percent <= 90%%; got percents: %v",
			func() []float64 {
				out := make([]float64, len(rippingEvents))
				for i, p := range rippingEvents {
					out[i] = p.Percent
				}
				return out
			}())
	}

	// EstimatedTotal should be set on ripping events.
	if first.EstimatedTotal <= 0 {
		t.Errorf("EstimatedTotal = %d, want > 0", first.EstimatedTotal)
	}

	// Final complete event must have Percent == 100.
	if last.Status != "complete" {
		t.Fatalf("last status = %q, want complete", last.Status)
	}
	if last.Percent != 100 {
		t.Errorf("final percent = %v, want 100", last.Percent)
	}
}

// #99: a probe failure after a version was seen is transient noise (busy
// binary, ejected drive) and must log at DEBUG; only a failure with no
// version ever seen justifies the WARN.
func TestProbeFailureLogLevelAfterVersionSeen(t *testing.T) {
	origTO := probeTimeout
	t.Cleanup(func() { probeTimeout = origTO })
	probeTimeout = 300 * time.Millisecond

	tmp, err := os.CreateTemp(t.TempDir(), "stderr")
	if err != nil {
		t.Fatal(err)
	}
	defer tmp.Close()
	savedFD, err := syscall.Dup(syscall.Stderr)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { syscall.Dup2(savedFD, syscall.Stderr) })

	// A failing binary (non-zero exit, no banner).
	script := writeFakeMakemkv(t, "exit 3")

	// First: never-seen version → the failure must WARN.
	syscall.Dup2(int(tmp.Fd()), syscall.Stderr)
	m := NewMakeMKV(script)
	if m.IsAvailable() {
		t.Fatal("failing makemkvcon reported available")
	}
	tmp.Sync()
	data, _ := os.ReadFile(tmp.Name())
	if !bytes.Contains(data, []byte("level=WARN msg="+`"MakeMKV probe failed"`)) {
		t.Errorf("first failure should WARN, output:\n%s", data)
	}

	// Simulate a version having been seen earlier, then failing again.
	m.mu.Lock()
	m.version = "v2.0.0"
	m.mu.Unlock()
	tmp.Truncate(0)
	tmp.Seek(0, 0)
	m.check()
	tmp.Sync()
	data, _ = os.ReadFile(tmp.Name())
	if bytes.Contains(data, []byte("MakeMKV probe failed")) && !bytes.Contains(data, []byte("level=DEBUG msg="+`"MakeMKV probe failed (transient; version seen earlier)"`)) {
		t.Errorf("transient failure should not WARN, output:\n%s", data)
	}
}
