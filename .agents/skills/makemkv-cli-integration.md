---
name: makemkv-cli-integration
description: >
  Integration patterns for MakeMKV CLI (`makemkvcon`).
  Covers robot-mode parsing, drive state management, progress tracking,
  and error recovery.
metadata:
  language: rust
  version: "1.16+"
  compatibility: tokio, subprocess, regex
---

# MakeMKV CLI Integration

## General Principles
- **Robot Mode:** Always use `-r` flag for structured output.
- **Async:** Use `tokio::process` for non-blocking CLI interaction.
- **State Machine:** Implement strict drive state management.

## Drive State Machine
States: `UNKNOWN` → `EMPTY` → `LOADING` → `READY` → `RIPPING` → `COMPLETE`/`ERROR`/`CANCELLED`

- **UNKNOWN:** Initial state.
- **EMPTY:** No disc detected.
- **LOADING:** Disc detected, MakeMKV is identifying it (up to 5s timeout).
- **READY:** Disc is accessible, ready to rip.
- **RIPPING:** Rip session active.
- **COMPLETE:** Rip finished successfully.
- **ERROR:** Rip failed.
- **CANCELLED:** User cancelled rip.

## Parsing Robot Mode Output
Use regex to parse the following lines:
- **PRGV:** Progress (`PRGV:current,total,max`). Calculate progress based on phase.
- **PRGC:** Current operation (`PRGC:code,id,name`). Track title index.
- **MSG:** Messages (`MSG:code,flags,count,message`). Check for errors.
- **TCOUNT:** Total titles (`TCOUNT:N`).

## Progress Tracking
- **Analyzing Phase:**
  - Trigger: Before `PRGC:5017` ("Saving to MKV file").
  - Calculation: Track PRGV full cycles (0→65536 reset). Each cycle ≈ 30%.
  - Formula: `progress = cycles * 30 + (current/max) * 0.3`. Cap at 90%.
  - Monotonic: Never decrease.
- **Ripping Phase:**
  - Trigger: After `PRGC:5017`.
  - Calculation: Straight percentage from PRGV.
  - Formula: `progress = (current/max) * 100`.
  - Weighted: If multiple titles, use `progress = (title_idx * 100 + per_title_pct) / total_titles`.

## Safety
- Validate drive indices before issuing commands.
- Implement timeouts for long-running processes (e.g., 30s for disc identification).
- Handle MakeMKV errors gracefully (e.g., disc errors, drive errors).
- Eject disc on completion or error.

## Error Recovery
- Retry failed rips once.
- Log detailed error messages from MakeMKV.
- Allow user to cancel rip at any time.