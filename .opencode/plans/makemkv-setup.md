# MakeMKV Setup & Installation — Plan

## Status: BUILDING

---

## Problem

MakeMKV installation is currently a manual process. When `auto_detect()` fails:
- CLI prints a wall of text with multiple install options (source, flatpak, config)
- Web UI has no awareness — just returns empty drives / 503

We want a guided installation flow that:
1. Detects what's missing (flatpak, flathub, makemkv, cdrom group)
2. Offers to install via Flatpak (preferred, avoids unreliable source builds)
3. Checks drive access permissions and offers to fix them

---

## Design Decisions

1. **CLI owns installation, web shows status + points to CLI** — security boundary, no `sudo` from web requests
2. **Flatpak is the primary install path** — source install is too complex (two tarballs, interactive EULA, heavy build deps, forums unreliable)
3. **Interactive CLI flow** — check → confirm → install → verify, streaming output
4. **`GET /api/ripper/status`** — lightweight endpoint for web UI to show what's missing
5. **cdrom group** — check and offer to add with explanation of why it's needed
6. **`$SUDO_USER` over `$USER`** — when running with sudo, use the original user for `usermod`
7. **Flatpak permissions** — check `flatpak info --show-permissions` for `device=all`
8. **`install_makemkv()` is sync** — no async needed, it's the only thing running in CLI context
9. **Partial failure recovery** — Step 5 re-runs detection; on failure, offer retry of just failed step
10. **Dry-run support** — `install_makemkv(dry_run: bool)` prints what it would do without executing
11. **Network check** — quick connectivity test before flatpak operations, fail early if offline
12. **Banner refresh** — web UI tells user to refresh after running `riply setup`, no auto-poll

---

## New Module: `infra/src/makemkv/setup.rs`

### Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DetectionMethod {
    Native(String),  // path to binary
    Flatpak,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MissingComponent {
    Flatpak,
    Flathub,
    MakeMKV,
    CdromGroup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationStatus {
    pub installed: bool,
    pub method: DetectionMethod,
    pub flatpak_available: bool,
    pub flathub_configured: bool,
    pub drive_access_ok: bool,
    pub missing: Vec<MissingComponent>,
}
```

### Public API

```rust
/// Check installation status. Safe to call from web server (no side effects).
pub fn check_installation() -> InstallationStatus

/// Run interactive installation. CLI only — needs TTY for sudo prompts.
/// Synchronous — runs system commands and streams output to stdout.
/// If dry_run is true, prints what would be done without executing.
/// Returns final status.
pub fn install_makemkv(dry_run: bool) -> Result<InstallationStatus>
```

### Private Helpers

```rust
/// Detect available package manager. Returns install command template.
fn detect_package_manager() -> Option<PackageManager>

struct PackageManager {
    name: &'static str,          // "apt", "dnf", "pacman"
    install_flatpak: Vec<String>, // ["sudo", "apt", "install", "-y", "flatpak"]
}

fn check_flatpak() -> bool         // `which flatpak`
fn check_flathub() -> bool         // `flatpak remotes` contains "flathub"
fn check_cdrom_group() -> bool     // `id -nG $USER` contains "cdrom"
fn check_drive_permissions() -> bool // `flatpak info --show-permissions` contains "device=all"
fn check_network() -> bool         // quick connectivity test (flatpak remote-ls)
fn get_install_user() -> String    // $SUDO_USER || $USER — the user to add to cdrom group
```

### `check_installation()` Logic

```
1. Try MakemkvBackend::auto_detect() → sets method + installed
2. check_flatpak() → flatpak_available
3. if flatpak_available: check_flathub() → flathub_configured
4. check_cdrom_group() → drive_access_ok
5. Build missing[]:
   - if !flatpak_available → MissingComponent::Flatpak
   - if flatpak_available && !flathub_configured → MissingComponent::Flathub
   - if method == None → MissingComponent::MakeMKV
   - if !drive_access_ok → MissingComponent::CdromGroup
6. Return InstallationStatus
```

### `install_makemkv()` Flow

Interactive, CLI-only, synchronous. Each step: check → skip if done → offer to install.
If `dry_run` is true, prints what would be done without executing any commands.

```
Step 0: Network check
  if check_network() → proceed
  else → "[!] No network connection. Flatpak operations require internet.
          Check your connection and try again." → return Err

Step 1: Flatpak
  if check_flatpak() → "[✓] Flatpak already installed"
  else:
    detect_package_manager()
    if Some(pm) → "Flatpak is needed to install MakeMKV from Flathub.
                   It provides a sandboxed runtime with all dependencies bundled."
                   Install Flatpak? [Y/n]
                   (dry_run: print "Would run: sudo $pm install -y flatpak")
                   → run sudo $pm.install_flatpak, stream output
    if None → "Could not detect package manager. Install Flatpak manually:
               https://flatpak.org/setup/"

Step 2: Flathub
  if check_flathub() → "[✓] Flathub already configured"
  else:
    "Flathub is the app store for Flatpak. MakeMKV is distributed through it."
    Add Flathub? [Y/n]
    (dry_run: print "Would run: flatpak remote-add --user --if-not-exists flathub ...")
    → run flatpak remote-add --user --if-not-exists flathub ...

Step 3: MakeMKV
  if Flatpak detection finds com.makemkv.MakeMKV → "[✓] MakeMKV already installed"
  else:
    "MakeMKV is the disc ripping tool. Install from Flathub?"
    [Y/n]
    (dry_run: print "Would run: flatpak install -y flathub com.makemkv.MakeMKV")
    → run flatpak install -y flathub com.makemkv.MakeMKV, stream output

Step 4: Cdrom group
  if check_cdrom_group() → "[✓] User is in cdrom group"
  else:
    let user = get_install_user()  // $SUDO_USER || $USER
    "Your user account ({user}) needs cdrom group membership to access optical drives.
     Linux restricts /dev/sr0 (DVD/Blu-ray drives) to root:cdrom.
     Without this, MakeMKV can find the drive but cannot read disc contents.
     
     This requires sudo (admin) access to add your user to the group."
    Add {user} to cdrom group? [Y/n]
    (dry_run: print "Would run: sudo usermod -aG cdrom {user}")
    → run sudo usermod -aG cdrom {user}
    → warn: "Log out and back in (or reboot) for the group change to take effect."

Step 5: Verify
  → call MakemkvBackend::auto_detect()
  → if Ok → "[✓] Setup complete! Run `riply rip list` to see drives."
  → if Err → "[!] Installation completed but auto-detect still failed.
              You may need to configure the command in config.toml:
              [ripping]
              command = \"flatpak run --command=makemkvcon com.makemkv.MakeMKV\""
```

---

## CLI Changes

### New file: `cli/src/commands/setup.rs`

Implements the `riply setup` command. Calls into `infra::setup::install_makemkv()`.

### Modified: `cli/src/commands/rip.rs`

Replace the wall of install instructions in `get_backend()` with:

```
No ripping backend found. Run `riply setup` to install MakeMKV.
```

### Modified: `cli/src/main.rs` (or wherever subcommands are registered)

Add `Setup` variant to the command enum.

---

## Web Changes

### New endpoint: `GET /api/ripper/status`

**`web/src/api.rs`**:

```rust
async fn ripper_status(State(state): State<Arc<AppState>>) -> Json<ApiResponse<InstallationStatus>> {
    let status = infra::setup::check_installation();
    Ok(Json(ApiResponse::ok(status)))
}
```

Wire into router: `.route("/api/ripper/status", get(ripper_status))`

### Modified: `web/svelte/src/routes/ripping/+page.svelte`

Add a banner at the top of the ripping page when `GET /api/ripper/status` returns `installed: false`:

```
┌──────────────────────────────────────────────────┐
│  MakeMKV is not installed                        │
│                                                  │
│  Run `riply setup` in your terminal to install   │
│  MakeMKV via Flatpak.                            │
│                                                  │
│  Missing: Flatpak, MakeMKV                       │
│  Drive access: needs cdrom group                 │
└──────────────────────────────────────────────────┘
```

The `missing` array provides detail — the frontend can show exactly what's needed, but the action is always "run riply setup".

---

## Files Summary

| File | Action | Description |
|------|--------|-------------|
| `infra/src/makemkv/setup.rs` | **CREATE** | `InstallationStatus`, `check_installation()`, `install_makemkv()`, pkg manager detection |
| `infra/src/makemkv/mod.rs` | MODIFY | Add `pub mod setup` + re-exports |
| `cli/src/commands/setup.rs` | **CREATE** | `riply setup` interactive command |
| `cli/src/commands/rip.rs` | MODIFY | Replace install instructions with "Run `riply setup`" |
| `cli/src/main.rs` | MODIFY | Register `Setup` subcommand |
| `web/src/api.rs` | MODIFY | Add `GET /api/ripper/status` endpoint |
| `web/svelte/src/routes/ripping/+page.svelte` | MODIFY | Add setup banner component |

---

## Verification

```bash
cargo fmt
cargo clippy --all
cargo build --all
cargo test --all
```

Manual testing:
1. `cargo run -p cli -- setup` — should detect missing components and offer install
2. `cargo run -p cli -- rip list` — should show "Run `riply setup`" when not installed
3. `cargo run -p cli -- serve` → curl localhost:3000/api/ripper/status → should return InstallationStatus
4. Web UI ripping page should show banner when MakeMKV not installed

---

## Commit

```
feat(infra): add MakeMKV installation checker and interactive setup

- Add setup.rs with check_installation() and install_makemkv()
- Detect package managers (apt/dnf/pacman) for flatpak installation
- Check flatpak, flathub, makemkv, and cdrom group membership
- Interactive CLI setup command with step-by-step installation
- Add GET /api/ripper/status endpoint for web UI
- Show setup banner on ripping page when not installed
- Simplify rip command error to point to riply setup
```
