package logger

import (
	"bytes"
	"context"
	"log/slog"
	"os"
	"strings"
	"syscall"
	"testing"
)

// captureStderr redirects the process stderr fd to a temp file for the
// duration of fn and returns what was written.
func captureStderr(t *testing.T, fn func()) string {
	t.Helper()
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
	syscall.Dup2(int(tmp.Fd()), syscall.Stderr)
	fn()
	tmp.Sync()
	data, err := os.ReadFile(tmp.Name())
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

func TestDebugSilentWithoutRiplyDebug(t *testing.T) {
	// debug logger is nil when RIPLY_DEBUG != "1" (package init).
	if os.Getenv("RIPLY_DEBUG") == "1" {
		t.Skip("test requires RIPLY_DEBUG unset")
	}
	out := captureStderr(t, func() { Debug("hidden message", "k", "v") })
	if strings.Contains(out, "hidden message") {
		t.Error("Debug leaked to stderr without RIPLY_DEBUG=1")
	}
}

func TestInfoWarnErrorAlwaysLog(t *testing.T) {
	out := captureStderr(t, func() {
		Info("info line", "k", 1)
		Warn("warn line")
		Error("error line")
	})
	for _, want := range []string{"level=INFO msg=\"info line\" k=1", "level=WARN msg=\"warn line\"", "level=ERROR msg=\"error line\""} {
		if !strings.Contains(out, want) {
			t.Errorf("missing %q in:\n%s", want, out)
		}
	}
}

func TestDebugHandlerLogsAndAddsSource(t *testing.T) {
	// Rebuild the debug handler the way init() does with RIPLY_DEBUG=1.
	var buf bytes.Buffer
	debugLocal := slog.New(slog.NewTextHandler(&buf, &slog.HandlerOptions{Level: slog.LevelDebug, AddSource: true}))
	debugLocal.DebugContext(context.Background(), "dbg line")
	out := buf.String()
	for _, want := range []string{"level=DEBUG", `msg="dbg line"`, "source="} {
		if !strings.Contains(out, want) {
			t.Errorf("debug output missing %q: %q", want, out)
		}
	}
}
