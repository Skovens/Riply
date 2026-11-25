package rip

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)


// --- makemkvcon execution-path tests (#103) ---

// Arg construction: --minlength present only when configured, robot mode and
// output dir always present.
func TestRipArgumentConstruction(t *testing.T) {
	argLog := filepath.Join(t.TempDir(), "args.log")
	script := writeFakeMakemkv(t, "echo \"ARGS:$*\" >> "+argLog+"; echo 'MSG:5011,0,0,\"Operation successfully completed\"' >> "+argLog)
	m := NewMakeMKV(script)

	// With minlength set: the flag must appear.
	m.SetMinTitleLength(120)
	for range m.Rip(context.Background(), "/dev/sr0", t.TempDir(), 0) {
	}
	data, err := os.ReadFile(argLog)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), "--minlength=120") || !strings.Contains(string(data), "-r mkv disc:0 all") {
		t.Errorf("args wrong with minlength: %s", data)
	}

	// With minlength cleared: the flag must be omitted (MakeMKV default).
	m.SetMinTitleLength(0)
	os.Remove(argLog)
	for range m.Rip(context.Background(), "/dev/sr0", t.TempDir(), 0) {
	}
	data, err = os.ReadFile(argLog)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(data), "--minlength") {
		t.Errorf("args contain --minlength although disabled: %s", data)
	}
	if !strings.Contains(string(data), "-r mkv disc:0 all") {
		t.Errorf("args missing robot-mode mkv invocation: %s", data)
	}
}

// Cancelling the context must close the progress channel promptly (well
// under a second) without invoking the real binary.
func TestRipCancellationClosesChannel(t *testing.T) {
	script := writeFakeMakemkv(t, `echo 'MSG:1005,0,0,"MakeMKV v2.0.0 by www.makemkv.com"'; exec sleep 30`)
	m := NewMakeMKV(script)

	ctx, cancel := context.WithCancel(context.Background())
	ch := m.Rip(ctx, "/dev/sr0", t.TempDir(), 0)
	time.Sleep(300 * time.Millisecond) // let makemkvcon start
	cancel()

	deadline := time.After(1 * time.Second)
	for {
		select {
		case _, ok := <-ch:
			if !ok {
				return // channel closed: clean cancel
			}
		case <-deadline:
			t.Fatal("progress channel not closed within 1s of cancellation")
		}
	}
}

// Non-zero exit code must surface as an error progress event, not a silent
// success.
func TestRipNonZeroExitIsError(t *testing.T) {
	script := writeFakeMakemkv(t, `echo 'PRGV:0,1,1'; exit 7`)
	m := NewMakeMKV(script)

	var sawError bool
	for p := range m.Rip(context.Background(), "/dev/sr0", t.TempDir(), 0) {
		if p.Status == "error" {
			sawError = true
		}
	}
	if !sawError {
		t.Error("non-zero exit did not produce an error progress event")
	}
}

// RIPLY_MAKEMKV_LOG captures the raw robot-mode lines of the info run.
func TestRawCaptureWritesRobotLines(t *testing.T) {
	capture := filepath.Join(t.TempDir(), "capture.log")
	t.Setenv("RIPLY_MAKEMKV_LOG", capture)

	script := writeFakeMakemkv(t, `echo 'MSG:1005,0,0,"MakeMKV v2.0.0 by www.makemkv.com"'`)
	m := NewMakeMKV(script)
	m.GetDiscInfo(context.Background(), "/dev/sr0")

	data, err := os.ReadFile(capture)
	if err != nil {
		t.Fatalf("raw capture not written: %v", err)
	}
	if !strings.Contains(string(data), "MSG:1005") {
		t.Errorf("capture missing MSG line: %s", data)
	}
}
