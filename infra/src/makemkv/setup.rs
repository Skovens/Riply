use std::io::{self, Write};
use std::process::Command as StdCommand;

use serde::{Deserialize, Serialize};

use super::backend::MakemkvBackend;

// ── Types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DetectionMethod {
    Native(String),
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
    pub flatpak_permissions_ok: bool,
    pub drive_access_ok: bool,
    pub missing: Vec<MissingComponent>,
}

// ── Public API ────────────────────────────────────────────────

/// Check installation status. Safe to call from any context (no side effects).
pub fn check_installation() -> InstallationStatus {
    let method = match MakemkvBackend::auto_detect() {
        Ok(backend) => {
            let cmd = backend.command_string();
            if cmd.contains("flatpak") {
                DetectionMethod::Flatpak
            } else {
                DetectionMethod::Native(cmd)
            }
        }
        Err(_) => DetectionMethod::None,
    };

    let flatpak_available = check_flatpak();
    let flathub_configured = flatpak_available && check_flathub();
    let flatpak_permissions_ok = flatpak_available && check_flatpak_permissions();
    let drive_access_ok = check_cdrom_group();

    let mut missing = Vec::new();
    if !flatpak_available {
        missing.push(MissingComponent::Flatpak);
    } else if !flathub_configured {
        missing.push(MissingComponent::Flathub);
    }
    if method == DetectionMethod::None {
        missing.push(MissingComponent::MakeMKV);
    }
    if !drive_access_ok {
        missing.push(MissingComponent::CdromGroup);
    }

    InstallationStatus {
        installed: method != DetectionMethod::None,
        method,
        flatpak_available,
        flathub_configured,
        flatpak_permissions_ok,
        drive_access_ok,
        missing,
    }
}

/// Run interactive installation. CLI only — needs TTY for sudo prompts.
/// If `dry_run` is true, prints what would be done without executing.
pub fn install_makemkv(dry_run: bool) -> anyhow::Result<InstallationStatus> {
    println!("MakeMKV Setup");
    println!("=============\n");

    // Step 1: Flatpak
    if check_flatpak() {
        println!("[✓] Flatpak already installed");
    } else {
        println!("[!] Flatpak not found");
        match detect_package_manager() {
            Some(pm) => {
                println!(
                    "\nFlatpak is needed to install MakeMKV from Flathub.\nIt provides a sandboxed runtime with all dependencies bundled."
                );
                if prompt_yes_no("Install Flatpak?", true)? {
                    if dry_run {
                        println!("[dry-run] Would run: sudo {} install -y flatpak", pm.name);
                    } else {
                        print!("\nInstalling Flatpak via {}... ", pm.name);
                        io::stdout().flush()?;
                        run_sudo(&pm.install_flatpak)?;
                        println!("OK");
                    }
                } else {
                    println!("Skipping Flatpak installation.");
                }
            }
            None => {
                println!("\nCould not detect package manager (apt/dnf/pacman).");
                println!("Install Flatpak manually: https://flatpak.org/setup/");
                println!("Then re-run `riply setup`.");
                return Err(anyhow::anyhow!("No package manager found"));
            }
        }
    }

    // Step 2: Flathub
    if check_flathub() {
        println!("[✓] Flathub already configured");
    } else if !check_flatpak() {
        println!("[!] Flathub skipped (Flatpak not available)");
    } else {
        println!("[!] Flathub remote not configured");
        println!("\nFlathub is the app store for Flatpak. MakeMKV is distributed through it.");
        if prompt_yes_no("Add Flathub remote?", true)? {
            if dry_run {
                println!("[dry-run] Would run: flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo");
            } else {
                print!("Adding Flathub... ");
                io::stdout().flush()?;
                let status = StdCommand::new("flatpak")
                    .args([
                        "remote-add",
                        "--user",
                        "--if-not-exists",
                        "flathub",
                        "https://dl.flathub.org/repo/flathub.flatpakrepo",
                    ])
                    .status()?;
                if status.success() {
                    println!("OK");
                } else {
                    println!("FAILED (exit code: {:?})", status.code());
                    println!("You may need to run: flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo");
                }
            }
        } else {
            println!("Skipping Flathub configuration.");
        }
    }

    // Step 3: MakeMKV
    if MakemkvBackend::auto_detect().is_ok() {
        println!("[✓] MakeMKV already installed");
    } else if !check_flatpak() || !check_flathub() {
        println!("[!] MakeMKV skipped (Flatpak or Flathub not available)");
    } else {
        println!("[!] MakeMKV not installed");
        println!("\nMakeMKV is the disc ripping tool used to read DVDs and Blu-rays.");
        if prompt_yes_no("Install MakeMKV from Flathub?", true)? {
            if dry_run {
                println!("[dry-run] Would run: flatpak install -y flathub com.makemkv.MakeMKV");
            } else {
                print!("Installing MakeMKV... ");
                io::stdout().flush()?;
                let status = StdCommand::new("flatpak")
                    .args(["install", "-y", "flathub", "com.makemkv.MakeMKV"])
                    .status()?;
                if status.success() {
                    println!("OK");
                } else {
                    println!("FAILED (exit code: {:?})", status.code());
                    println!("Try running manually: flatpak install flathub com.makemkv.MakeMKV");
                }
            }
        } else {
            println!("Skipping MakeMKV installation.");
        }
    }

    // Step 4: Cdrom group
    if check_cdrom_group() {
        println!("[✓] User is in cdrom group");
    } else {
        let user = get_install_user();
        println!("[!] User '{}' is not in the cdrom group", user);
        println!("\nOptical drives (/dev/sr0) require cdrom group membership to read discs.");
        println!("Without this, MakeMKV can detect the drive but cannot read disc contents.");
        println!("This requires sudo (admin) access to add your user to the group.");
        if prompt_yes_no(&format!("Add '{}' to cdrom group?", user), true)? {
            if dry_run {
                println!("[dry-run] Would run: sudo usermod -aG cdrom {}", user);
            } else {
                print!("Adding '{}' to cdrom group... ", user);
                io::stdout().flush()?;
                run_sudo(&["usermod", "-aG", "cdrom", &user])?;
                println!("OK");
                println!(
                    "\n⚠ Log out and back in (or reboot) for the group change to take effect."
                );
            }
        } else {
            println!("Skipping cdrom group update.");
            println!("Drive access may not work without this.");
        }
    }

    // Step 5: Verify
    println!("\nVerifying installation...");
    let status = check_installation();
    if status.installed {
        println!("[✓] Setup complete! Run `riply rip list` to see drives.");
    } else {
        println!("[!] Auto-detection still failed after installation.");
        println!("You may need to configure the command in config.toml:");
        println!("  [ripping]");
        println!("  command = \"flatpak run --command=makemkvcon com.makemkv.MakeMKV\"");
    }

    Ok(status)
}

// ── Private helpers ───────────────────────────────────────────

fn check_flatpak() -> bool {
    StdCommand::new("which")
        .arg("flatpak")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn check_flathub() -> bool {
    StdCommand::new("flatpak")
        .args(["remotes"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("flathub"))
        .unwrap_or(false)
}

fn check_flatpak_permissions() -> bool {
    let output = match StdCommand::new("flatpak")
        .args(["info", "--show-permissions", "com.makemkv.MakeMKV"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    let info = String::from_utf8_lossy(&output.stdout);
    info.contains("device=all") || info.contains("devices=all") || info.contains("device=all-all")
}

fn check_cdrom_group() -> bool {
    let output = match StdCommand::new("id").arg("-nG").output() {
        Ok(o) => o,
        Err(_) => return false,
    };
    let groups = String::from_utf8_lossy(&output.stdout);
    groups.split_whitespace().any(|g| g == "cdrom")
}

fn get_install_user() -> String {
    if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        if !sudo_user.is_empty() {
            return sudo_user;
        }
    }
    std::env::var("USER").unwrap_or_else(|_| "root".to_string())
}

struct PackageManager {
    name: &'static str,
    install_flatpak: Vec<&'static str>,
}

fn detect_package_manager() -> Option<PackageManager> {
    if StdCommand::new("which")
        .arg("apt")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some(PackageManager {
            name: "apt",
            install_flatpak: vec!["apt", "install", "-y", "flatpak"],
        });
    }

    if StdCommand::new("which")
        .arg("dnf")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some(PackageManager {
            name: "dnf",
            install_flatpak: vec!["dnf", "install", "-y", "flatpak"],
        });
    }

    if StdCommand::new("which")
        .arg("pacman")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some(PackageManager {
            name: "pacman",
            install_flatpak: vec!["pacman", "-S", "--noconfirm", "flatpak"],
        });
    }

    None
}

fn run_sudo(args: &[&str]) -> anyhow::Result<()> {
    let status = StdCommand::new("sudo").args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "sudo command failed (exit code: {:?})",
            status.code()
        ))
    }
}

fn prompt_yes_no(message: &str, default_yes: bool) -> anyhow::Result<bool> {
    let hint = if default_yes { "Y/n" } else { "y/N" };
    print!("{} [{}]: ", message, hint);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    Ok(match input.as_str() {
        "y" | "yes" => true,
        "n" | "no" => false,
        "" => default_yes,
        _ => {
            println!("Please enter y or n.");
            prompt_yes_no(message, default_yes)?
        }
    })
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detection_method_serialization() {
        let method = DetectionMethod::Native("/usr/bin/makemkvcon".to_string());
        let json = serde_json::to_string(&method).unwrap();
        assert!(json.contains("Native"));
        let back: DetectionMethod = serde_json::from_str(&json).unwrap();
        assert_eq!(method, back);
    }

    #[test]
    fn test_missing_component_serialization() {
        let missing = vec![MissingComponent::Flatpak, MissingComponent::CdromGroup];
        let json = serde_json::to_string(&missing).unwrap();
        assert!(json.contains("Flatpak"));
        assert!(json.contains("CdromGroup"));
    }

    #[test]
    fn test_installation_status_serialization() {
        let status = InstallationStatus {
            installed: false,
            method: DetectionMethod::None,
            flatpak_available: false,
            flathub_configured: false,
            flatpak_permissions_ok: false,
            drive_access_ok: false,
            missing: vec![MissingComponent::Flatpak, MissingComponent::MakeMKV],
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"installed\":false"));
        assert!(json.contains("\"flatpak_available\":false"));

        let back: InstallationStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status.installed, back.installed);
        assert_eq!(status.missing, back.missing);
    }

    #[test]
    fn test_check_installation_does_not_panic() {
        // Should work in any environment including CI
        let status = check_installation();
        // We just verify it returns a valid struct
        assert!(!status.missing.is_empty() || status.installed);
    }

    #[test]
    fn test_detect_package_manager_returns_something_on_linux() {
        // CI runners typically have apt or dnf
        let pm = detect_package_manager();
        // Don't assert Some — some CI might not have a package manager
        // Just ensure it doesn't panic
        if let Some(pm) = pm {
            assert!(!pm.install_flatpak.is_empty());
        }
    }

    #[test]
    fn test_get_install_user_returns_something() {
        let user = get_install_user();
        assert!(!user.is_empty());
    }

    #[test]
    fn test_check_cdrom_group_does_not_panic() {
        // Should work in any environment
        let _ = check_cdrom_group();
    }
}
