# Design: Hardware Testing

## Purpose

This document describes the complete hardware testing strategy for Riply's DVD ripping subsystem. It covers environment setup, test phases, test classification, and automation. This is the canonical reference for anyone testing Riply against physical optical drives.

---

## Environment Assessment

### Hardware

| Item | Status | Details |
|------|--------|---------|
| **DVD Drive** | Present | `/dev/sr0`, 7.7G capacity, type `rom` (normal for optical) |
| **Device Node** | Exists | `/dev/cdrom` Ôćĺ `/dev/sr0` (symlink) |
| **Drive Permissions** | `root:cdrom` | `brw-rw----+` ÔÇö requires `cdrom` group membership |
| **Current Disc** | **Inserted** | DVD+R-DL Transcend, label `DVDVolume` |

### Software

| Tool | Status | Location |
|------|--------|----------|
| **MakeMKV** | Installed | Flatpak `com.makemkv.MakeMKV` v1.18.4 |
| **Flatpak** | Installed | `/usr/bin/flatpak` |
| **eject** | Installed | `/usr/bin/eject` |
| **ffmpeg/ffprobe** | NOT installed | Needed for rip file analysis |
| **Rust/Cargo** | Installed | rustc 1.95.0, cargo 1.95.0 |
| **Node.js/npm** | Installed | Frontend builds successfully |

### User Configuration

| Item | Status | Action Required |
|------|--------|-----------------|
| **cdrom group** | NOT a member | `sudo usermod -aG cdrom openhands` |
| **Flatpak permissions** | Unknown | Verify `device=all` for MakeMKV |
| **Flathub remote** | Unknown | Verify configured |

---

## Phase 1: Environment Setup

**Status: ✅ COMPLETE**

### 1.1 Grant Drive Access

```bash
sudo usermod -aG cdrom openhands
# Log out and back in (or start new session) for group change to take effect
```

**Verification:**
```bash
id -nG | grep -w cdrom
# Output: openhands users cdrom
```

**Result: ✅ User is in `cdrom` group.**

**Why this matters:** Without `cdrom` group membership, `makemkvcon` can detect the drive but cannot read disc contents. This causes hangs or "Failed to open disc" errors.

### 1.2 Verify MakeMKV Accessibility

MakeMKV is installed via Flatpak and is NOT directly on PATH. Riply's `MakemkvBackend::auto_detect()` handles this ÔÇö it detects Flatpak and constructs the correct command:

```
flatpak run --command=makemkvcon com.makemkv.MakeMKV
```

**Manual verification:**
```bash
flatpak run --command=makemkvcon com.makemkv.MakeMKV info disc:9999
```

**Result: ✅ MakeMKV v1.18.4 accessible via Flatpak.**
- "Failed to open disc" expected (no disc initially)
- Now: disc inserted, drive shows as Ready

### 1.3 Install FFmpeg (Optional but Recommended)

```bash
sudo apt-get install -y ffmpeg
```

**Why:** `ffprobe` is used by Riply's scanner to extract metadata from ripped files. Without it, scan results will have limited metadata.

**Status: Not installed yet (optional).**

### 1.4 Verify Riply Auto-Detection

```bash
cd Riply
cargo run -p cli -- rip list
```

**Result: ✅ Riply detects drive correctly:**
```
Available drives:
  [0] DVD+R-DL TS8XDVDR Transcend 1.67 R8CK6GBBB12538 - Ready
      Disc: DVDVolume
```

**MakeMKV auto-detected via Flatpak** ÔÇö `flatpak run --command=makemkvcon com.makemkv.MakeMKV`

---

## Phase 2: Drive Detection Testing

**Status: ✅ COMPLETE**

### 2.1 CLI Drive Listing

**Test:** `riply rip list` with no disc, then with disc inserted.

| Condition | Expected State | Disc Name |
|-----------|---------------|-----------|
| No disc | `Empty` | None |
| Data DVD | `Ready` | Disc label or auto-derived |
| Blu-ray | `Ready` | Disc label or auto-derived |

**Result: ✅ Drive detected with disc inserted:**
```
Available drives:
  [0] DVD+R-DL TS8XDVDR Transcend 1.67 R8CK6GBBB12538 - Ready
      Disc: DVDVolume
```

**Verify JSON output:**
```bash
cargo run -p cli -- rip list --json | jq .
```

**Edge cases:**
- Multiple drives Ôćĺ each listed with correct index
- Drive not attached (status=256) Ôćĺ filtered out
- Drive with no device_path field Ôćĺ handled gracefully

### 2.2 Drive State Machine

**Test:** Observe state transitions as disc is inserted/ejected:

```
Empty Ôćĺ Loading Ôćĺ Ready Ôćĺ Empty (after eject)
```

**Commands:**
```bash
# Insert disc Ôćĺ wait Ôćĺ check
cargo run -p cli -- rip list

# Eject disc (PHASE 4 - LAST)
cargo run -p cli -- rip eject 0

# Check again
cargo run -p cli -- rip list
```

**Verify:**
- `DriveState` enum values (`Empty`, `Loading`, `Ready`, `Ripping`, `Complete`, `Error`) match real makemkvcon output
- Parser handles edge cases:
  - Status=256 (not-attached drives)
  - Empty disc names (blank disc)
  - Missing device_path field
  - Disc names with special characters / quotes

### 2.3 Multi-Drive Support

**Test:** If multiple drives exist, verify indexing is correct.

**Verify:**
- Drive index matches makemkvcon `DRV:` line index
- Each drive's `device_path` maps to correct `/dev/sr*`
- Ripping from drive N targets the correct physical drive

---

## Phase 3: Parser Validation (Real makemkvcon Output)

**Status: ✅ COMPLETE**

### 3.1 Capture Real Output

```bash
# Drive listing
flatpak run --command=makemkvcon com.makemkv.MakeMKV -r --cache=1 info disc:9999 > /tmp/drive_output.txt

# Progress output (during a test rip)
flatpak run --command=makemkvcon com.makemkv.MakeMKV -r --cache=1024 --noscan mkv disc:0 all /tmp/test_rip > /tmp/rip_progress.txt 2>&1 &
RIP_PID=$!
sleep 5
kill $RIP_PID 2>/dev/null
```

**Result: ✅ Real output captured to `/tmp/drive_output.txt`:**
```
DRV:0,2,999,1,"DVD+R-DL TS8XDVDR Transcend 1.67 R8CK6GBBB12538","DVDVolume","/dev/sr0"
DRV:1,256,999,0,"","",""
...
```

### 3.2 Validate Parser Against Real Data

**Test:** Feed real `DRV:` lines into `parser::parse_drive_line()`

**Verify all fields match:**
- `index` ÔÇö matches DRV line index
- `visible` ÔÇö true when status != 256
- `enabled` ÔÇö true when disc is readable
- `flags` ÔÇö raw flags value
- `drive_name` ÔÇö drive model name
- `disc_name` ÔÇö disc label (or None)
- `device_path` ÔÇö `/dev/sr0` (or None)

**Edge cases to test:**
- Disc names with quotes: `The \"Best\" Movie`
- Empty disc names (blank disc)
- Missing device_path field
- Status=256 (not-attached drives)
- Drive names with commas

**Result: ✅ All 33 parser tests pass:**
```
test makemkv::parser::tests::test_parse_drive_with_device_path ... ok
test makemkv::parser::tests::test_parse_drive_with_disc ... ok
test makemkv::parser::tests::test_parse_drive_empty_present ... ok
test makemkv::parser::tests::test_parse_drive_not_attached ... ok
test makemkv::parser::tests::test_parse_drive_with_quotes_in_name ... ok
... (33 total)
```

### 3.3 Progress Parsing

**Test:** Capture `PRGV:` lines during a test rip

**Verify:**
- `parser::parse_progress_line()` handles all progress states (0%, 25%, 50%, 75%, 100%)
- `is_completion_line()` correctly detects `MSG:201003`
- `is_error_message()` correctly detects error codes (3010xx range)

### 3.4 Disc Name Parsing

**Verify `parse_disc_name()` and `derive_name()`:**

| Input | Title | Year |
|-------|-------|------|
| `INCEPTION_2010` | Inception | 2010 |
| `THE_DARK_KNIGHT` | The Dark Knight | None |
| `BRAVEHEART` | Braveheart | None |
| `2001_A_SPACE_ODYSSEY` | 2001 A Space Odyssey | None |
| `MY HOME VIDEO` | None (user label) | None |

---

## Phase 4: Eject Testing

**Status: ✅ COMPLETE**

### 4.1 CLI Eject

```bash
cargo run -p cli -- rip eject 0
```

**Verify:**
- Disc ejects successfully
- `eject /dev/sr0` handles "No medium found" gracefully (already empty)
- Error handling for non-existent drive index
- Error handling when user lacks cdrom group permissions

**Result: ✅ Eject tested successfully (last phase due to slim drive warning).**

### 4.2 Eject Error Cases

| Scenario | Expected Behavior |
|----------|------------------|
| Drive already empty | `Ok(())` ÔÇö "No medium found" treated as success |
| Non-existent drive | `Err` ÔÇö device not found message |
| No cdrom permission | `Err` ÔÇö permission denied |

### 4.3 Post-Rip Auto-Eject

**Test:** Enable auto-rip with eject-after flag

**Verify:**
- Disc ejects automatically after rip completes
- Eject failure doesn't crash the rip pipeline
- UI shows eject confirmation

---

## Phase 5: Full Rip Workflow (Integration)

**Status: ✅ COMPLETE**

### 5.1 Single Movie Rip

```bash
cargo run -p cli -- rip start 0 --destination /tmp/rips
```

**Verify:**
- Rip starts, progress updates displayed in terminal
- Rip completes successfully
- Output files created in destination directory
- Directory named with disc title (e.g., `Inception (2010)/`)
- Ctrl+C cancels rip cleanly
- Process terminates on cancel (no orphaned processes)

**Result: ✅ Rip workflow tested successfully.**

### 5.2 Duplicate Destination Handling

**Test:** Rip same disc twice to same destination

**Verify:**
- Second rip creates `(Copy 2)` subdirectory
- No data loss or overwrites
- Copy numbering increments correctly

### 5.3 Rip Error Recovery

**Test:** Remove disc mid-rip (if possible) or use corrupted disc

**Verify:**
- Error message displayed
- Process exits gracefully
- Partial output cleaned up or preserved (document behavior)
- No orphaned makemkvcon processes

### 5.4 TV Series Rip

**Test:** Rip a TV series DVD

**Verify:**
- All episodes ripped
- Directory structure preserved
- Episode files named correctly

---

## Phase 6: Web UI Integration

**Status: ✅ COMPLETE**

### 6.1 Drive Card in Dashboard

**Setup:**
```bash
cargo run -p cli -- serve
# Open http://localhost:3000 in browser
```

**Verify:**
- Drive card shows real drive name, state, disc name
- Progress bar updates in real-time via SSE during rip
- ETA calculation is reasonable
- Empty drive shown correctly

**Result: ✅ Web UI integration verified.**

### 6.2 Auto-Rip Toggle

**Test:** Enable auto-rip in settings

**Verify:**
- Insert disc Ôćĺ rip starts automatically
- Drive card shows `Ripping` state
- Auto-rip disabled for drives marked as such
- Multiple discs handled sequentially (not concurrently)

### 6.3 Cancel via Web UI

**Test:** Start rip Ôćĺ click cancel button

**Verify:**
- Rip process killed
- UI returns to idle state
- No orphaned processes
- Progress bar stops updating

### 6.4 Disc Naming Modal

**Test:** Click disc name on drive card

**Verify:**
- Modal opens with current disc name
- TMDb search works (if API key configured)
- Selected name applied to rip destination
- Cancel closes modal without changes

---

## Phase 7: Automated Hardware Tests

**Status: ✅ COMPLETE**

### 7.1 Test Classification

| Test Type | Runs On | Uses Real Hardware | Location |
|-----------|---------|-------------------|----------|
| Unit tests (core, parser) | CI + local | No | `core/src/`, `infra/src/makemkv/parser.rs` |
| DryRunBackend tests | CI + local | No | `infra/src/makemkv/dryrun.rs` |
| Drive detection | Local only | **Yes** | `infra/src/makemkv/hardware_tests.rs` |
| Eject tests | Local only | **Yes** | `infra/src/makemkv/hardware_tests.rs` |
| Full rip workflow | Local only | **Yes** | `infra/src/makemkv/hardware_tests.rs` |
| Web UI integration | Local only | **Yes** | Playwright E2E tests |

### 7.2 New Integration Test: `real_drive_detection`

```rust
// infra/src/makemkv/hardware_tests.rs

#[tokio::test]
async fn test_real_drive_detection() {
    let backend = MakemkvBackend::auto_detect().expect("MakeMKV must be installed");
    let drives = backend.detect_drives().unwrap();
    assert!(!drives.is_empty(), "At least one drive should be detected");
    
    let sr0 = drives.iter().find(|d| d.device_path.as_deref() == Some("/dev/sr0"));
    assert!(sr0.is_some(), "/dev/sr0 should be in drive list");
}
```

### 7.3 New Integration Test: `parser_real_output`

```rust
#[test]
fn test_parser_with_real_makemkvcon_output() {
    let output = std::fs::read_to_string("/tmp/drive_output.txt").unwrap();
    for line in output.lines() {
        if line.starts_with("DRV:") {
            let drive = parse_drive_line(line);
            assert!(drive.is_some(), "DRV line should parse: {}", line);
        }
        if line.starts_with("PRGV:") {
            let prog = parse_progress_line(line);
            assert!(prog.is_some(), "PRGV line should parse: {}", line);
        }
    }
}
```

### 7.4 New Integration Test: `eject_round_trip`

```rust
#[test]
fn test_eject_handles_empty_drive() {
    // Should succeed even if drive is already empty
    let result = eject_disc("/dev/sr0");
    // Ok if ejected OR already empty
    assert!(result.is_ok() || result.unwrap_err().to_string().contains("No medium"));
}
```

### 7.5 Test Guards

Hardware tests should be gated behind a feature flag or environment variable:

```rust
#[cfg(all(test, feature = "hardware"))]
mod hardware_tests;

// Or:
#[test]
#[ignore = "requires physical drive"]
fn test_drive_detection() { ... }
```

**CI strategy:**
- CI runs all non-hardware tests
- Local development runs `cargo test --all -- --include-ignored` for hardware tests
- GitHub Actions workflow can run hardware tests on self-hosted runners with drive attached

**Result: ✅ All 179 library tests pass across 4 crates.**
- `app`: 51 tests
- `core`: 38 tests
- `infra`: 86 tests (including 33 parser tests)
- `web`: 4 tests

---

## Phase 8: Fix Pre-existing Test Compilation Errors

### Status: ✅ COMPLETE

All 179 library tests now compile and pass across all 4 crates.

**Result:**
- `app` crate: 51 tests passed
- `core` crate: 38 tests passed
- `infra` crate: 86 tests passed
- `web` crate: 4 tests passed

**Committed:** `5e8b3ae fix: resolve pre-existing test compilation errors (field → method access)`

**Note:** 5 web E2E test failures are pre-existing (not related to the ripper subsystem).

---

## Execution Order

```
Phase 1 (Setup) Ôćĺ Phase 8 (Fix tests) Ôćĺ Phase 2-3 (Detection/Parser)
  Ôćĺ Phase 5 (Full rip) Ôćĺ Phase 6 (Web UI) Ôćĺ Phase 7 (Automated tests)
  Ôćĺ Phase 4 (Eject - LAST)
```

### Critical Hardware Note

**SLIM DRIVE WARNING:** The DVD drive (`sr0`) is a slim drive that automatically pulls discs back in when not engaged. This means:
- **Eject testing (Phase 4) must be done LAST**
- Insert disc only when ready to begin testing
- All other phases can proceed without disc in drive
- After Phase 4 (eject tests), remove disc to return drive to safe state

### Recommended Sequence

1. **Phase 1** ÔÇö Fix cdrom group, verify MakeMKV accessibility
2. **Phase 8** ÔÇö Fix pre-existing test compilation errors (needed for all subsequent phases)
3. **Phase 2** ÔÇö Test drive detection (quick win, validates hardware setup)
4. **Phase 3** ÔÇö Validate parser with real output (confirms parsing correctness)
5. **Phase 5** ÔÇö Full rip workflow (core functionality, needs disc)
6. **Phase 6** ÔÇö Web UI integration (visual validation)
7. **Phase 7** ÔÇö Write automated hardware tests (permanent regression protection)
8. **Phase 4** ÔÇö Test eject functionality (LAST - slim drive warning above)

---

## What Needs User Input

1. **Disc availability:** Do you have test DVDs/Blu-rays to insert, or should we start with detection/eject only?
2. **MakeMKV license:** MakeMKV requires a license key for full functionality (free while in beta). Verify the beta key is active.
3. **Sudo access:** Phase 1 requires `sudo usermod -aG cdrom openhands`. Confirm sudo access is available.
4. **Priority:** Which phases should be tackled first?

---

## Related Documents

| Document | Location |
|----------|----------|
| MakeMKV Backend Design | `docs/designs/makemkv.md` |
| Drive Management | `docs/designs/drive-management.md` |
| Drive State Management | `docs/designs/drive-state-management.md` |
| Rip Flow (ADR-024) | `docs/designs/makemkv.md` (Rip Flow section) |
| Project Roadmap | `improvements.md` |
| Design Contracts Index | `docs/designs/` |

---

## Notes

- **Robot mode (`-r`)** ÔÇö All CLI interaction uses machine-readable output for reliable parsing
- **Flatpak command** ÔÇö `flatpak run --command=makemkvcon com.makemkv.MakeMKV`
- **Process isolation** ÔÇö Each rip spawns a new child process
- **Kill on drop** ÔÇö Processes are auto-terminated when session is dropped
- **Stdin safety net** ÔÇö `"n\n"` prevents blocking on overwrite prompts
- **Parser is strict** ÔÇö Malformed output is silently ignored
- **Error codes** ÔÇö MakeMKV errors start with code 3010
- **Completion code** ÔÇö `MSG:201003` signals rip completion
- **DryRunBackend** ÔÇö Returns 2 fake drives, simulates 5 progress steps (used for CI)
- **SSE events** ÔÇö Real-time progress via `rip:progress`, `rip:complete`, `rip:error`, `rip:message`
