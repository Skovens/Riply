package logger

import (
	"context"
	"log/slog"
	"os"
)

var (
	info  *slog.Logger
	debug *slog.Logger
)

func init() {
	// INFO always goes to stderr
	info = slog.New(slog.NewTextHandler(os.Stderr, nil))

	// DEBUG goes to stderr with file:line source info
	if os.Getenv("RIPLY_DEBUG") == "1" {
		debug = slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{
			Level:     slog.LevelDebug,
			AddSource: true,
		}))
	}
}

func Info(msg string, args ...any) {
	if len(args) > 0 {
		info.InfoContext(context.Background(), msg, args...)
	} else {
		info.InfoContext(context.Background(), msg)
	}
}

func Debug(msg string, args ...any) {
	if debug != nil {
		if len(args) > 0 {
			debug.DebugContext(context.Background(), msg, args...)
		} else {
			debug.DebugContext(context.Background(), msg)
		}
	}
}

func Warn(msg string, args ...any) {
	if len(args) > 0 {
		info.WarnContext(context.Background(), msg, args...)
	} else {
		info.WarnContext(context.Background(), msg)
	}
}

func Error(msg string, args ...any) {
	if len(args) > 0 {
		info.ErrorContext(context.Background(), msg, args...)
	} else {
		info.ErrorContext(context.Background(), msg)
	}
}
