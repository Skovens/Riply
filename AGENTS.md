# Riply — Agentic Coding Guidelines

> Read `README.md` for project overview, architecture, API reference, and limitations.

## Tech Stack

- **Go 1.26+** with `net/http` standard library only
- `github.com/BurntSushi/toml` v1.6.0 — TOML config
- Vanilla HTML + CSS + JS — no frontend framework, embedded into the binary with `embed`
- MakeMKV external binary (`makemkvcon`) via `os/exec`
- Drive state is in-memory; completed rips (disc name, TMDb match, season/episode) persist alongside scanned files in the SQLite library index (`modernc.org/sqlite`, pure Go) at `$XDG_CONFIG_HOME/riply/library.db` and drive Auto-Sequence across restarts

## Debugging

**Preferred agent workflow:** drive Delve through the DAP debugger at `xd://debug` (Oh My Pi's single-tool DAP surface). This presents program state directly in Go — breakpoints, stepped execution, variable inspection, expression evaluation — without relying on log tailing first.

Project adapter config lives in `.omp/dap.json`; it pins the `dlv` adapter and sets Riply-friendly launch/attach defaults.

- Install Delve: `go install github.com/go-delve/delve/cmd/dlv@latest`
- Only one DAP root session can exist at a time. Start a new one only after terminating the old (`{ "action": "terminate" }`) or killing the process.
- Launch `riply` under the debugger (`./cmd/server` is a package directory, so Delve uses `mode: "debug"`):
  ```json
  { "action": "launch", "adapter": "dlv", "program": "./cmd/server", "timeout": 120 }
  ```
  (The first launch builds the binary; use `timeout: 120` or longer so the request doesn't time out.)
- Attach to an already-running `riply` process:
  ```json
  { "action": "attach", "adapter": "dlv", "pid": <PID> }
  ```
- Typical inspection flow once stopped:
  1. `set_breakpoint` — `file` + `line` (or `function`)
  2. `continue`
  3. `stack_trace` / `threads`
  4. `scopes` for a frame, then `variables` for a scope
  5. `evaluate` arbitrary Go expressions in the stopped context
- Other useful actions: `step_over`, `step_in`, `step_out`, `pause`, `remove_breakpoint`, `output` (cached stdout/stderr), `terminate`.

When logs are still useful, use `RIPLY_DEBUG=1` for per-line DEBUG output.

Manual Delve fallback (for humans or non-DAP shells):
- Launch: `dlv debug ./cmd/server`
- Attach: `dlv attach <PID>`
- Headless DAP server: `dlv dap --listen=127.0.0.1:40000` (then attach remotely)
- Helper script: `scripts/debug.sh {launch|attach|headless}`

## Module Layout

See `README.md` for the full architecture diagram. Key modules:

| Module | Purpose |
|--------|---------|
| `internal/config` | TOML loading, defaults |
| `internal/drive` | Drive scanning, state machine |
| `internal/rip` | MakeMKV implementation |
| `internal/naming` | TMDb HTTP client (rate-limited) |
| `internal/move` | File organization |
| `internal/sse` | Event broadcast |
| `internal/metrics` | Atomic counters |
| `internal/library` | SQLite library history, scan, next-up |

## Coding Conventions

### Error Handling
- Always check errors from `os/exec`, `os.ReadFile`, and HTTP calls
- Return descriptive errors: `fmt.Errorf("failed to eject drive %d: %w", idx, err)`
- Never use `panic` — the server should stay running even if one drive fails

### HTTP Handlers (`internal/api/`)
- Use `http.NewServeMux()` with Go 1.22+ pattern matching
- **Never** introduce chi, gorilla, or any HTTP framework beyond stdlib
- Check `ripper == nil || !ripper.IsAvailable()` before any MakeMKV operation
- Return 503 when MakeMKV is unavailable, 400 for bad input, 404 for missing resources
- Parse request bodies with `json.NewDecoder(r.Body).Decode(&body)`
- Write responses with `writeJSON(w, data)` helper

### SSE Events (`internal/sse/`)
- Always publish via `sseMgr.Publish(Event{...})` — never write directly to channels
- Event types: `drive_loading`, `drive_ready`, `drive_ejected`, `rip_progress`, `rip_complete`, `rip_failed`, `rip_cancelled`, `rip_accepted`, `rip_duplicate`, `naming_started`, `naming_match`, `naming_failed`, `drive_accepted`, `library_scanned`, `library_scan_failed`, `library_reset`, `shutdown`
- The `naming_*` events are emitted by `startAutoNaming` in `internal/api/api.go`; `rip_duplicate` by `duplicateWarning` in `internal/api/library.go`
- Include `Progress` struct with `Title`, `Percent`, `SpeedMBPS` for progress events

### Drive State Machine (`internal/drive/`)
- Only use defined states: `NoDisc`, `Loading`, `Ready`, `Ripping`, `Complete`, `Error`
- State transitions must be strict — no skipping states
- Drive detection polls every 2s, probing `CDROM_DRIVE_STATUS` via ioctl with a sysfs fallback (`internal/drive/poller.go`)

### MakeMKV Integration (`internal/rip/`)
- Always check `IsAvailable()` before any MakeMKV operation
- Use `makemkvcon -r` (robot mode) for structured output parsing
- Parse `PRGV:` for progress, `MSG:5017` for title completion, `MSG:5011` for rip complete
- Ripping runs in a background goroutine with progress streamed via SSE
- Handle `makemkvcon` exit codes gracefully — don't crash the server
- Diagnostics switches: `RIPLY_DEBUG=1` (per-line DEBUG logs with `file:line`) and `RIPLY_MAKEMKV_LOG=<path>` (raw robot-mode capture appended to a file)

### Config (`internal/config/`)
- Config lives at `$XDG_CONFIG_HOME/riply/config.toml` (default `~/.config/riply/config.toml`), auto-created on first run
- Use `config.Load()` which falls back to defaults — never panic on missing config
- Config is saved to disk via `config.Save()` — changes persist across restarts; `http_host`, `http_port` and `makemkv_path` only take effect on restart
- `tmdb_api_key` is optional — auto-naming is disabled if empty

### Frontend (`web/`)
- Vanilla HTML + CSS + JS only — no framework additions
- Dark theme using CSS custom properties
- SSE connection via `EventSource` to `/events`
- Auto-reconnect on connection loss

## Git Workflow

- **One branch + one PR per feature or fix.** Before starting work, create a branch: `git switch -c feat/<topic>` or `fix/<topic>` (never commit directly to `main`).
- The PR is the review surface: CI (build + vet + `go test -race`) gates the change before it reaches `main`, and the PR serves as permanent history for what changed and why.
- **CI must pass before a task is done.** Before pushing, run locally exactly what CI runs (`go build ./...`, `go vet ./...`, `go test -race ./...`) so the first push is expected green. After pushing, watch the run (`gh pr checks <N> --watch`) and fix any failure before reporting completion — a red run means the task is not finished. Never merge on a failing check.
- Do not switch branches with uncommitted changes in the tree — commit or ask first; a switch would yank files out from under an in-progress session.
- Push and open the PR with `gh pr create` once the change is verified; merge via PR, never by pushing to `main` directly.
- **Branch hygiene: a merged branch is a dead branch.** After a PR merges (whether merged locally or via the API), delete both sides — `git branch -d <branch>` locally and `git push origin --delete <branch>` remotely (or merge with `gh pr merge --delete-branch`). Before deleting, confirm the branch is fully merged (`git branch -r --merged origin/main`); never delete unmerged work. A repo cluttered with merged `feat/*`/`fix/*` branches hides which work is actually in flight.

## Verification Philosophy

This is a 1-person team — verify on real hardware with real discs. `.github/workflows/ci.yml` runs build + vet + `go test -race` on every push/PR; there are no Dockerfiles.

- **`go test ./...` must pass** before considering a change done (`internal/api`, `internal/naming`, `internal/rip`)
- **Verify correctness by reading log output** — use `logger.Info` at key decision points
- **For complex state machines** (drive states, rip lifecycle), add log statements at each state transition
- **If a bug is found**, the fix should include a new log statement that would have caught it
- When adding new features, include `logger.Info` calls that demonstrate the feature working
## When Unsure, Log First
- **Never guess** — if you're uncertain about a code path, state, or data value, add a log statement to verify it before writing code
- Add `logger.Debug`/`logger.Info` at decision points, function entry/exit, and state transitions to gather evidence (`RIPLY_DEBUG=1` enables the DEBUG lines)
- If you find yourself making assumptions about what the code does, instrument it first, then act on the logs
- This prevents circular guessing loops and saves time

## Testing Hygiene

- **Always kill old riply instances before starting new ones** — check with `ps aux | grep riply`, then `kill <PID>`
- **Use a consistent startup pattern**: `pkill -f "./riply" 2>/dev/null; ./riply > /tmp/riply.log 2>&1 &`
- **Never leave riply running in the background** after testing — it holds the port and can cause stale state
- **Check for lingering processes** if the server won't start or responds with old behavior

Example:
```go
logger.Info("Drive discovered", "path", event.Path)
logger.Debug("classifyRip", "type", mediaType, "median_mb", medianMB)
```

## Anti-Patterns to Avoid

- **Don't add new CI configs or Dockerfiles** — the existing GitHub Actions workflow (build + vet + `go test -race`) covers CI; Docker is out of scope
- **TV handling already exists** (`IsTV`, `SeasonNumber`/`EpisodeNumber`, `Season N/` paths) and must keep working; new TV-specific features are out of scope
- **Don't introduce dependencies beyond `BurntSushi/toml` and `modernc.org/sqlite`**
- **Don't add databases beyond the library history** — drive state stays in memory, completed rips persist as `library` rows via `internal/library`, and no second history table is introduced
- **Don't use `fmt.Printf` or the stdlib `log` package** — use `riply/internal/logger` (`logger.Info`/`Debug`/`Warn`/`Error`) for timestamped, structured logs
- **Don't block the HTTP handler goroutine** — use `go` for long-running operations

## Versioning

- The project version is hard-coded in `internal/version/version.go` as `0.0.1`.
- **Do not bump this version number** (or add a `version` subcommand/CI-driven
  bump) unless explicitly asked to by the user.

## Cross-References

- **Known limitations**: See `README.md` — do not re-list here
- **API endpoints**: See `README.md` — do not duplicate tables here
- **Configuration fields**: See `README.md` — do not re-document here
- **Architecture**: See `README.md` — do not duplicate diagrams here
