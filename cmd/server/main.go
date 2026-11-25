package main

import (
	"context"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"runtime"
	"strconv"
	"syscall"
	"time"

	"riply/internal/api"
	"riply/internal/config"
	"riply/internal/drive"
	"riply/internal/library"
	"riply/internal/logger"
	"riply/internal/makemkvinstall"
	"riply/internal/metrics"
	"riply/internal/naming"
	"riply/internal/rip"
	"riply/internal/sse"
)

func main() {
	// Load config — uses XDG path (~/.config/riply/config.toml) by default
	cfgPath := config.DefaultConfigPath()
	cfg, err := config.Load(cfgPath)
	if err != nil {
		logger.Warn("config load failed", "err", err)
		cfg = config.Default()
	}

	// Initialize components
	scanner := drive.New()
	sseMgr := sse.New()
	m := &metrics.Metrics{}

	// MakeMKV
	var ripper rip.Ripper
	makemkvPath := cfg.Makemkv
	if makemkvPath == "" {
		makemkvPath = "makemkvcon"
	}
	ripper = rip.NewMakeMKV(makemkvPath)
	if setter, ok := ripper.(rip.MinLengthSetter); ok {
		setter.SetMinTitleLength(cfg.MinTitleLength)
	}

	if !ripper.IsAvailable() {
		// Fresh-machine offer: MakeMKV is missing, so suggest installing it.
		// Interactive terminals get a Y/n prompt; anything else (a service
		// launch, a piped stdin) only sees the manual steps, so an
		// unattended server is never blocked on a TTY read.
		ripper = maybeInstallMakeMKV(ripper, makemkvPath, cfg.MinTitleLength)
	}

	if !ripper.IsAvailable() {
		logger.Warn("MakeMKV not found — rip functionality disabled")
	} else {
		logger.Info("MakeMKV available", "version", ripper.Version())
	}

	// TMDb — always construct the client so a key saved later via Settings
	// takes effect without a restart; empty key means naming is disabled.
	namer := naming.NewClient(cfg.TMDBKey)
	if cfg.TMDBKey != "" {
		logger.Info("TMDb client configured")
	} else {
		logger.Info("TMDb naming disabled", "reason", "no tmdb_api_key configured")
	}

	// Library history — SQLite index of ripped and scanned media. A broken
	// database must never take the server down; history is disabled then.
	libPath := filepath.Join(filepath.Dir(cfgPath), "library.db")
	lib, err := library.Open(libPath)
	if err != nil {
		logger.Warn("Library database unavailable; history disabled", "path", libPath, "error", err)
		lib = nil
	} else {
		logger.Info("Library database available", "path", libPath)
	}
	defer func() {
		if lib != nil {
			_ = lib.Close()
		}
	}()
	server := api.New(cfg, cfgPath, scanner, ripper, namer, sseMgr, m, lib)

	// Setup routes
	mux := http.NewServeMux()
	server.RegisterRoutes(mux)

	// Start drive scanning
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Start drive/disc detection.
	poller := drive.NewPoller()
	eventCh := make(chan drive.DiscEvent, 64)
	go poller.Run(ctx, eventCh)

	// Feed events to scanner and handle disc title lookup
	go drive.NewEventLoop(ctx, scanner, ripper, sseMgr, server, eventCh).Run()

	// Start HTTP server
	addr := net.JoinHostPort(cfg.HTTPHost, strconv.Itoa(cfg.HTTPPort))
	if !isLoopbackHost(cfg.HTTPHost) {
		logger.Warn("Riply has no authentication — do not expose this address", "host", cfg.HTTPHost)
	}
	srv := &http.Server{
		Addr:              addr,
		Handler:           mux,
		ReadHeaderTimeout: 10 * time.Second,
		// M7: cap client send time on bodies/keep-alive reads. WriteTimeout
		// stays 0 deliberately — SSE streams would be killed by it.
		ReadTimeout: 30 * time.Second,
	}

	// Open browser
	go func() {
		time.Sleep(500 * time.Millisecond)
		url := fmt.Sprintf("http://localhost:%d", cfg.HTTPPort)
		logger.Info("Opening browser", "url", url)
		openBrowser(url)
	}()

	logger.Info("Riply starting", "addr", addr)

	// Run ListenAndServe in a goroutine so we can listen for signals.
	errCh := make(chan error, 1)
	go func() {
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			errCh <- err
		}
		close(errCh)
	}()

	// Listen for SIGINT/SIGTERM.
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
	select {
	case err := <-errCh:
		logger.Error("HTTP server", "error", err)
		os.Exit(1)
	case sig := <-sigCh:
		logger.Info("shutdown requested", "signal", sig)
	}

	// Graceful shutdown.
	shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer shutdownCancel()

	if err := server.Shutdown(shutdownCtx); err != nil {
		logger.Error("shutdown timed out", "error", err)
		os.Exit(1)
	}

	// Shutdown HTTP server.
	if err := srv.Shutdown(shutdownCtx); err != nil {
		logger.Error("HTTP shutdown", "error", err)
	}

	// Stop drive detection and cancel the drive context.
	poller.Stop()
	cancel()

	logger.Info("shutdown complete")
	os.Exit(0)
}

// maybeInstallMakeMKV is the fresh-machine MakeMKV offer. When the ripper
// has no working MakeMKV, it offers to install it; on a successful install
// it returns a fresh ripper so the new binary is probed (the original
// instance cached its negative verdict at construction). On a declined
// offer, a non-interactive terminal, or a failed install it returns the
// original ripper unchanged, so rip functionality stays disabled but the
// server still starts.
func maybeInstallMakeMKV(original rip.Ripper, makemkvPath string, minTitleLength int) rip.Ripper {
	if makemkvinstall.Prompt() {
		if err := makemkvinstall.RunInstall(); err != nil {
			logger.Warn("MakeMKV install failed", "error", err)
			fmt.Println()
			fmt.Println(makemkvinstall.ManualInstructions())
			return original
		}
		logger.Info("MakeMKV installed — re-probing")
		var fresh rip.Ripper = rip.NewMakeMKV(makemkvPath)
		if setter, ok := fresh.(rip.MinLengthSetter); ok {
			setter.SetMinTitleLength(minTitleLength)
		}
		if fresh.IsAvailable() {
			return fresh
		}
		logger.Warn("MakeMKV still unavailable after install — check that makemkvcon is on PATH")
	}
	fmt.Println()
	fmt.Println(makemkvinstall.ManualInstructions())
	return original
}

// isLoopbackHost reports whether the bind host only accepts connections from
// this machine. Empty binds all interfaces (not loopback). "localhost" and any
// name are resolved; every resolved address must be loopback — including
// IPv4-mapped IPv6 such as ::ffff:127.0.0.1, which net.ParseIP reports via
// IsLoopback once parsed.
func isLoopbackHost(host string) bool {
	if host == "" {
		return false
	}
	if ip := net.ParseIP(host); ip != nil {
		return ip.IsLoopback()
	}
	addrs, err := net.LookupHost(host)
	if err != nil || len(addrs) == 0 {
		return false
	}
	for _, a := range addrs {
		ip := net.ParseIP(a)
		if ip == nil || !ip.IsLoopback() {
			return false
		}
	}
	return true
}

func openBrowser(url string) {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "linux":
		cmd = exec.Command("xdg-open", url)
	case "darwin":
		cmd = exec.Command("open", url)
	case "windows":
		cmd = exec.Command("cmd", "/c", "start", "", url)
	default:
		return
	}
	if err := cmd.Start(); err != nil {
		logger.Warn("Browser open failed", "url", url, "error", err)
	}
}
