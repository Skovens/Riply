# ADR-023: Per-Drive Autorip State Is Session-Scoped

## Status

Accepted

## Context

Riply has a per-drive autorip toggle. `POST /v1/ripper/autorip/:index/toggle`
adds/removes a drive index in `state.autorip_drives` (an in-memory
`RwLock<HashSet<usize>>` in `AppState`, `web/src/api.rs`), `GET
/v1/ripper/autorip` reports the enabled set, and the `autorip_poll_loop`
starts rips only on drives in that set. The set is initialized empty on
every server start and is never written to disk; the only durable
autorip-related setting is the global `ripping.autorip_eject_after`
config flag.

The question is whether per-drive autorip activation should persist
across service restarts.

## Decision

Per-drive autorip activation is intentionally **session-scoped**: it must
not persist across a service restart. After any restart, no drive has
autorip enabled; the operator re-enables it explicitly per drive.

Rationale:

- A restart (crash, update, reboot) must never cause the service to
  spontaneously begin ripping discs that happen to be present in drives.
  Ripping is a long-running, hardware-binding operation (ADR-009,
  ADR-010); an unattended restart-triggered rip could ruin media, fill
  the disk, or surprise the user.
- Fresh session = clean state: the operator consciously opts in each
  time the service runs.
- Consistency with the ephemeral-state philosophy: operational runtime
  state (AGENTS.md gotcha #5) is disposable; durable state is limited to
  the database and the config file.

Alternatives considered and rejected:

- Persisting the enabled-drive set in the DB or in `config.toml`. This
  would resume autorip across restarts, contradicting the goal.
  Additionally, drive indices are positional and can shift between
  reboots, making persisted indices fragile.

## Consequences

- No surprise rips after a restart; the service never starts ripping on
  its own.
- Minor friction: the operator re-enables autorip per drive after each
  restart.
- `ripping.autorip_eject_after` remains durable because it is a behavior
  preference, not an activation state.
- If autorip persistence is ever wanted later, it should key on a stable
  drive identity (serial/device path) and require an explicit opt-in -
  deliberately out of scope today.
