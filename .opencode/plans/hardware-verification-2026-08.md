# Plan: Full Hardware Verification (Dune 1984 disc)

## Status: PENDING APPROVAL

> Companion to `docs/designs/hardware-testing.md` (the static design). This plan
> is the *executable* test sequence for 2026-08-31 against the Transcend slim
> drive (sr0) with the **1984 Dune** disc.
>
> **Hardware constraint:** the drive can eject but cannot retract the tray —
> retraction is manual. The plan is ordered so the tray goes up only at the very
> end. If it sticks, worst case the user manually retracts once and the session
> is done.

## Verified preconditions (checked 2026-08-31)

| Item | State |
|---|---|
| Disc | 1984 Dune, label `DVDVolume`, `drv0 Ready` via `makemkvcon info disc:9999` |
| MakeMKV | 1.18.4 via Flatpak, auto-detected by Riply (`flatpak run --command=makemkvcon com.makemkv.MakeMKV`) |
| User | `openhands` is in `cdrom` group — drive readable |
| Toolchain | rustc 1.95.0, node 18.19.1, ffmpeg/ffprobe present |
| Eject code | `infra::eject_disc()` runs `eject <device>`; "No medium found" = Ok. Manual rip path **never** ejects; only autorip does, gated by `ripping.autorip_eject_after` (config, mutable at runtime via `/v1/config`) |
| Autorip trigger | 5s poll loop; fires on `Empty→Ready` OR first sighting of a Ready drive — works whether the disc is already in or inserted while watching |

## Phase 0 — Build & config (no disc needed)

1. `npm run build` in `web/svelte` (gotcha #10: embedded frontend goes stale after pull), then
   `cargo build --release -p cli`.
2. `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`,
   `cargo build --all`, `cargo test --all` (expect ~220 pass). **Record baseline.**
3. Write `config.toml` (gitignored): `tmdb_api_key`, `omdb_api_key` (from user's
   `TMDB_KEY`/`OMDB_KEY` — not visible in agent env, user must confirm how to pass),
   `scan_directory`, `movies_directory`, `rip_output_directory`, `temp_directory=/tmp/riply`,
   `port=3000`, `ripping.autorip_eject_after=false`.
4. Verify disk space ≥ 10 GB free for rips.

## Phase 1 — Baseline: detection & health (drive empty, tray down)

| # | Check | Method | Pass |
|---|---|---|---|
| 1.1 | Drive listed, empty state | `riply rip list` + `--json` | `drv0` present, `Empty` |
| 1.2 | MakeMKV status endpoint | `GET /v1/ripper/status` | `installed=true`, Flatpak |
| 1.3 | Web `/ripper` renders real drive | `serve` + browser, screenshot | drive name/model shown, Empty |
| 1.4 | SSE connects | browser devtools / curl `/v1/sse` | stream opens, no immediate close |

**User action: insert Dune disc here (Phase 2 is first).**

## Phase 2 — Manual rip, CLI (eject path NOT exercised)

1. `riply rip list` → `Ready`, disc name `DVDVolume`.
2. `riply rip start 0 --destination /tmp/riply/hwtest1` — watch progress until
   ~30 s: PRGV lines parse, % monotonic, ETA finite. Then **Ctrl+C** → assert
   no orphaned `makemkvcon`/`flatpak` process (this is the cancel-rip code path).
3. Full rip: `riply rip start 0 --destination /tmp/riply/hwtest1` → to completion.
   Verify: output files exist & non-zero; `ffprobe` shows video stream;
   `mkv` container valid (single main title + extras expected for a movie DVD).
4. `riply rip list` → still `Ready` (manual path must not eject).

## Phase 3 — Server + web UI (manual rip + autorip + eject, in one rip)

1. `riply serve` in background (config from Phase 0). Browser:
   - `/ripper`: drive card `Ready`, disc label visible.
   - **Manual rip from UI**: start → progress bar + ETA render from SSE
     (regression of the 2026-08-13 fix) → screenshot mid-rip.
   - **Cancel**: start second UI rip, cancel at ~20 s → UI back to idle,
     no orphan process, progress bar gone. (Then let nothing run — we don't
     need this rip's output.)
   - **Autorip toggle**: flip on for drive 0 (UI + `GET /v1/ripper/autorip`
     reflects it).
2. **Set `autorip_eject_after=true`** via `PUT /v1/config` (runtime, no restart).
3. **User action: insert disc (tray already up from Phase 2 rip).**
   Wait ≤10 s: autorip fires (first-sighting rule) → rip to `/tmp/riply/0` →
   auto-naming attempt (disc label `DVDVolume` — expect `naming_failed`, by
   design; Organizer is the naming surface) → post-rip move to
   `rip_output_directory` → **post-rip EJECT** → SSE `autorip.ejected` →
   drive state `Empty`.
4. Assert: no orphan process; output folder exists with files; tray is up (user
   sees it). **If the tray does NOT retract, the user pushes it in — session
   is then fully done; everything remaining in Phase 4 needs no disc.**

## Phase 4 — Pipeline with ripped media (no disc needed)

Uses the Phase 3 output folder (real Dune 1984 MKV on disk).

| # | Check | Method | Pass |
|---|---|---|---|
| 4.1 | Organizer scan | `POST /v1/folders/scan` on `rip_output_directory` | folder card appears with file list |
| 4.2 | Auto-detection on label | folder status | `Unknown` (label ≠ title — expected, documents behavior) |
| 4.3 | Manual TMDb search | `POST /v1/folders/:id/analyze` / web search | Dune (1984) found, high confidence |
| 4.4 | Accept | `POST /v1/folders/:id/accept` | status `Confident`/accepted |
| 4.5 | Move preview | `GET /v1/folders/:id/preview` | target = Jellyfin-named path under `movies_directory` |
| 4.6 | Move | `POST /v1/folders/:id/move` | files on disk at named destination; DB updated |
| 4.7 | CLI scan/analyze/review/move parity | `riply scan <dir>` on a sample dir, `analyze`, `review --yolo`, `move` | same results as API (CLI/web share `app/`) |
| 4.8 | `riply status` | CLI | counts match DB |

## Phase 5 — Eject edge cases (LAST — tray goes up)

1. **Empty-drive eject** (drive currently empty after Phase 3):
   `riply rip eject 0` → `Ok` ("No medium found" tolerated). UI eject button
   shows success, not an error toast.
2. *(Only if user wants belt-and-braces: full post-rip eject path with a second
   autorip — skipped by default since Phase 3 already exercised it.)*
3. Leave drive empty, tray down (user closes it). `riply rip list` → `Empty`.

## Out of scope (needs hardware/discs we don't have)

- Mid-rip disc removal / corrupted-disc error recovery (Phase 5.3 of the
  hardware-testing design) — needs a second disc.
- TV-series DVD (episode naming, multi-title structure).
- Multi-drive indexing (single physical drive).
- Slim-drive `Loading` state transition (unobservable while tray is closed;
  parser coverage exists in unit tests).

## Known risks / open questions for the user

1. **API keys**: `TMDB_KEY`/`OMDB_KEY` are not in my environment. How should
   I get them? (Paste here, or a file path — I will only write them to the
   gitignored `config.toml`.)
2. **"Cannot retract" behavior**: confirm the tray physically stays UP until
   pushed, and the disc remains inside until then. (Affects whether a stuck
   tray ends the session or just that one step.)
3. **Timing**: a full DVD rip can take 20–60 min depending on content size.
   Phase 3 (server UI + autorip + eject) is the long pole; Phase 4 needs no
   disc and runs afterward.

## Deliverables

- Findings log appended to this plan file as phases complete (pass/fail per row).
- Any bug found: fix → full suite → conventional commit (scope per AGENTS.md).
- Screenshots of web UI states for the final demo (AGENTS.md Step 4 format).

---

# Results (2026-08-31 session)

## Environment blocker — RESOLVED
- Symptom: makemkvcon reported "Operation successfully completed" but wrote 0 bytes; earlier real rip failed with `Posix error - No such file or directory` creating `A1_t00.mkv` in `/tmp/riply`.
- Root cause: MakeMKV runs in a Flatpak sandbox whose `filesystems=` grants only `xdg-videos` + `~/.MakeMKV:create` (read-only configs). No access to `/tmp`, which is Riply's default rip dir, so "saving" to `/tmp/riply` failed inside the sandbox.
- Fix applied (session, reversible): `flatpak override --user --filesystem=/tmp/riply com.makemkv.MakeMKV`. After this makemkvcon writes to `/tmp/riply` fine (partial file appears during rip).
- Not a Riply logic bug, but a portability landmine: any rip dir outside `~/Videos`/`~/.MakeMKV` silently produces empty output until the override is granted. `check_installation` reports `flatpak_permissions_ok:true` only after the override and does not probe the actual dest (B4).

## Verified working (real hardware, Dune 1984)
- Drive detection: `riply rip list`, `/v1/ripper/drives`, `/v1/ripper/status` (Flatpak auto-detect). OK
- Full rip via API -> 3 valid MKVs (720x576 mpeg2video + AC3 + DVD subs, ffprobe-confirmed, 7.95 GB) to `/tmp/riply/0`. OK
- Post-rip move: temp -> `organizer/DVDVolume` ("Post-rip move to Organizer completed"). OK
- Organizer scan: folder found, `file_count:3`, `main_file:A1_t00.mkv` (largest) chosen, `media_type:Movie` heuristic. OK
- SSE: `rip:starting` + 9 `rip:message` events stream in real time (JSON format). OK
- Eject (Phase 5, LAST): `riply rip eject 0` -> drive `Ready`->`Empty`; API eject `success`; eject-on-empty returns `Ok` (idempotent). OK

## Bugs found
- B1 (#10) — Cancel leaves orphaned makemkvcon. SIGINT to `riply rip start` -> `handle.cancel()` does `child.kill()` (SIGTERM) on the direct child (`flatpak`/`bwrap`); `bwrap` does not propagate to the `makemkvcon` grandchild, which reparents to PID 1 and keeps running. `kill_on_drop` only fires on drop, which never happens while riply holds the handle. Repro: start rip, Ctrl+C, `ps` shows orphan `bwrap ... makemkvcon -r ... mkv disc:0 all ...`. Fix: kill the whole process group on cancel (spawn in a new session + `kill -PID`) or bwrap-aware teardown.
- B2 (#11) — No live rip progress (UI progress bar/ETA dead on real rips). A full ~15 min rip produced 0 `PRGV:` lines (confirmed via RAW makemkvcon capture, not through Riply — 90s of active writing = 0 PRGV). Parser `parse_progress_line` is correct/unit-tested; the makemkvcon 1.18.4 build in this mode emits no progress, so `progress`/`eta` SSE events never fire and the web progress bar + ETA never update. Fix: (a) find the makemkvcon flag/mode that emits `PRGV:`; (b) robust fallback: derive pct from output-file-size growth (total size known from `Saving N titles`/title metadata). Self-contained in `process_rip_events`.
- B3 (#12) — Organizer move writes to a garbage path (final move non-functional). `app/src/organizer.rs` `move_folder` (L292/295) + `preview_move` (L333/338) hardcode literal strings `TODO_MOVIE_DIR` / `TODO_TV_DIR` for the destination instead of configured `movies_directory`/`tv_directory`. `OrganizerService::new(searcher, folder_store)` is never passed those dirs (web route `organizer.rs` constructs with `None`). Preview confirms: `GET /v1/folders/:id/preview` -> `TODO_MOVIE_DIR/DVDVolume (0)/`. A real move would `fs::rename` files to a literal relative dir, not the configured library. Fix: thread `movies_directory`/`tv_directory` from config into `OrganizerService` and build dest from them.
- B4 (#13) — `check_installation` `flatpak_permissions_ok` is misleading. Reports `true` when generic perms look right, but does not verify the actual rip destination is writable inside the sandbox (why the empty-output failure was silent until the override). Consider a destination-path writability probe.

## Not testable here (documented)
- TMDb/OMDb auto-naming + Organizer TMDb matching: API keys not reachable in this sandbox (only `GITHUB_TOKEN` is injected). Re-run Phase 4.3/4.4 (`analyze`->`accept` with real `suggested_name`) once `tmdb_api_key`/`omdb_api_key` are added to `config.toml`.
- Mid-rip disc removal / corrupted-disc recovery, TV-series DVD, multi-drive indexing, `Loading` state: no second disc / single physical drive / unobservable-with-closed-tray.

## State at session end
- Rip tray is OUT (post Phase-5 eject); operator must push it back in.
- `riply serve` still running; test rips live under `/tmp/riply/` (gitignored temp, auto-cleaned >7 days).
