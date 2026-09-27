use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use riply_core::{DriveInfo, DriveState, RipBackend, RipError, RipHandle};
use tracing::{debug, info};

use super::parser;
use super::session::MakemkvSession;

/// MakeMKV backend. Wraps `makemkvcon` CLI with configurable command.
#[derive(Debug)]
pub struct MakemkvBackend {
    command: Vec<String>,
}

/// Default parent directory for ripped content.
/// For Flatpak MakeMKV, this is `~/.var/app/com.makemkv.MakeMKV/riply`
/// (Flatpak namespace, host filesystem, plenty of space).
/// For native MakeMKV, it is `/tmp/riply`.
pub const DEFAULT_RIP_TEMP_DIR: &str = "/tmp/riply";
pub const FLATPAK_RIP_TEMP_DIR: &str = ".var/app/com.makemkv.MakeMKV/riply";

/// Resolve a default destination for a Flatpak MakeMKV rip.
/// Uses `~/.var/app/com.makemkv.MakeMKV/riply/{disc_name}` — inside the
/// Flatpak namespace on the host filesystem, avoiding /tmp tmpfs limits.
pub fn resolve_rip_destination_flatpak(disc_name: &str) -> (PathBuf, Option<String>) {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dest = PathBuf::from(home)
        .join(FLATPAK_RIP_TEMP_DIR)
        .join(disc_name);
    (dest, None)
}

/// Resolve a default destination for a native MakeMKV rip.
/// Uses `/tmp/riply/{disc_name}` for consistency.
pub fn resolve_rip_destination_native(disc_name: &str) -> (PathBuf, Option<String>) {
    (PathBuf::from(DEFAULT_RIP_TEMP_DIR).join(disc_name), None)
}

/// Check if a path is within Flatpak's default sandbox access.
pub fn is_flatpak_accessible(path: &Path) -> bool {
    let p = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    // /tmp is always accessible
    if p.starts_with("/tmp") {
        return true;
    }
    // ~/Documents, ~/Downloads, ~/.var/app
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        let flatpak_apps = home.join(".var/app");
        if p.starts_with(home.join("Documents"))
            || p.starts_with(home.join("Downloads"))
            || p.starts_with(&flatpak_apps)
        {
            return true;
        }
    }
    false
}

impl MakemkvBackend {
    /// Create a new backend with the given command.
    /// `command` can be a single binary or a full command line.
    pub fn new(command: Vec<String>) -> Self {
        Self { command }
    }

    /// Create a backend from a config string.
    /// Splits on whitespace (simple, handles most cases).
    pub fn from_config(command: &str) -> Self {
        let parts: Vec<String> = command.split_whitespace().map(String::from).collect();
        Self::new(parts)
    }

    /// Return the full command as a single string (for display/detection).
    pub fn command_string(&self) -> String {
        self.command.join(" ")
    }

    /// Check if this backend uses Flatpak.
    pub fn is_flatpak(&self) -> bool {
        self.command
            .first()
            .map(|c| c.contains("flatpak"))
            .unwrap_or(false)
    }

    /// Auto-detect makemkvcon installation.
    ///
    /// Tries in order:
    /// 1. `makemkvcon` on PATH (native install)
    /// 2. `/usr/bin/makemkvcon`
    /// 3. `/usr/local/bin/makemkvcon`
    /// 4. Flatpak: checks `flatpak list` for `com.makemkv.MakeMKV`
    pub fn auto_detect() -> Result<Self, RipError> {
        // Try native install on PATH
        if let Ok(output) = StdCommand::new("which").arg("makemkvcon").output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    info!(path = %path, "Found makemkvcon");
                    return Ok(Self::new(vec![path]));
                }
            }
        }

        // Try common paths
        for path in &["/usr/bin/makemkvcon", "/usr/local/bin/makemkvcon"] {
            if Path::new(path).exists() {
                info!(path = %path, "Found makemkvcon");
                return Ok(Self::new(vec![path.to_string()]));
            }
        }

        // Try Flatpak
        if StdCommand::new("flatpak")
            .args(["list"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("com.makemkv.MakeMKV"))
            .unwrap_or(false)
        {
            info!("Found MakeMKV via Flatpak");
            return Ok(Self::new(vec![
                "flatpak".to_string(),
                "run".to_string(),
                "--command=makemkvcon".to_string(),
                "com.makemkv.MakeMKV".to_string(),
            ]));
        }

        Err(RipError::NotAvailable)
    }

    /// Create a backend from config, falling back to auto-detection.
    pub fn from_config_or_detect(command: Option<&str>) -> Result<Self, RipError> {
        match command {
            Some(cmd) if !cmd.is_empty() => {
                info!(command = %cmd, "Using configured ripping command");
                Ok(Self::from_config(cmd))
            }
            _ => Self::auto_detect(),
        }
    }

    /// Run `makemkvcon` with the given args and return stdout.
    fn run_command(&self, args: &[&str]) -> Result<String, RipError> {
        debug!(command = %self.command.join(" "), args = %args.join(" "), "Running command");

        let output = StdCommand::new(&self.command[0])
            .args(&self.command[1..])
            .args(args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RipError::Backend(stderr.trim().to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

impl RipBackend for MakemkvBackend {
    fn detect_drives(&self) -> Result<Vec<DriveInfo>, RipError> {
        info!(command = %self.command.join(" "), "Detecting drives");

        let stdout = self.run_command(&["-r", "--cache=1", "info", "disc:9999"])?;
        let mut drives = Vec::new();

        for line in stdout.lines() {
            if let Some(parsed) = parser::parse_drive_line(line) {
                if parsed.visible {
                    let state = if parsed.disc_name.is_none() {
                        DriveState::Empty
                    } else if parsed.enabled {
                        DriveState::Ready
                    } else {
                        DriveState::Error
                    };

                    let derived_name = parsed
                        .disc_name
                        .as_deref()
                        .and_then(parser::derive_name)
                        .map(|(title, year)| match year {
                            Some(y) => format!("{title} ({y})"),
                            None => title,
                        });

                    drives.push(DriveInfo {
                        index: parsed.index,
                        name: parsed.drive_name,
                        disc_name: parsed.disc_name,
                        derived_name,
                        device_path: parsed.device_path,
                        state,
                    });
                }
            }
        }

        debug!(count = drives.len(), "Found drives");
        Ok(drives)
    }

    fn start_rip(&self, drive: usize, dest: &str) -> Result<RipHandle, RipError> {
        let dest_path = Path::new(dest);

        info!(drive, dest = %dest, "Starting rip");

        // For Flatpak MakeMKV, always create the directory inside the Flatpak namespace
        // because MakeMKV cannot see host-created directories in /tmp
        if self.is_flatpak() {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let flatpak_dest = PathBuf::from(home)
                .join(FLATPAK_RIP_TEMP_DIR)
                .join(dest_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default().as_ref());

            if !flatpak_dest.exists() {
                StdCommand::new("flatpak")
                    .args(["run", "--command=mkdir", "com.makemkv.MakeMKV", "-p", &flatpak_dest.display().to_string()])
                    .output()
                    .map_err(|e| RipError::Backend(format!("Failed to create directory: {}", e)))?;
            }

            let flatpak_dest_str = flatpak_dest.display().to_string();
            let child = tokio::process::Command::new(&self.command[0])
                .args(&self.command[1..])
                .args([
                    "-r",
                    "--cache=1024",
                    "mkv",
                    &format!("disc:{}", drive),
                    "all",
                    &flatpak_dest_str,
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()?;

            let session = MakemkvSession::new(child);
            return Ok(RipHandle::new(Box::new(session)));
        }

        // For native MakeMKV, use the original logic
        if !dest_path.exists() {
            if let Some(parent) = dest_path.parent() {
                if !parent.exists() {
                    return Err(RipError::DestinationParentMissing(
                        parent.display().to_string(),
                    ));
                }
                if !parent.canonicalize().map(|p| p.is_dir()).unwrap_or(false)
                    || !std::fs::write(parent.join(".riply_writability_test"), "test")
                        .or_else(|_| std::fs::remove_file(parent.join(".riply_writability_test")))
                        .is_ok()
                {
                    return Err(RipError::DestinationNotWritable(
                        parent.display().to_string(),
                    ));
                }
            }
        }

        if !dest_path.exists() {
            std::fs::create_dir_all(dest_path)?;
        }

        let child = tokio::process::Command::new(&self.command[0])
            .args(&self.command[1..])
            .args([
                "-r",
                "--cache=1024",
                "mkv",
                &format!("disc:{}", drive),
                "all",
                dest,
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let session = MakemkvSession::new(child);
        Ok(RipHandle::new(Box::new(session)))
    }

    fn is_sandboxed(&self) -> bool {
        self.is_flatpak()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_config_single_binary() {
        let backend = MakemkvBackend::from_config("makemkvcon");
        assert_eq!(backend.command, vec!["makemkvcon"]);
    }

    #[test]
    fn test_from_config_flatpak() {
        let backend =
            MakemkvBackend::from_config("flatpak run --command=makemkvcon com.makemkv.MakeMKV");
        assert_eq!(
            backend.command,
            vec![
                "flatpak",
                "run",
                "--command=makemkvcon",
                "com.makemkv.MakeMKV"
            ]
        );
    }

    #[test]
    fn test_auto_detect_returns_valid_result() {
        // auto_detect should work in any environment — find makemkvcon or not
        let result = MakemkvBackend::auto_detect();
        match result {
            Ok(backend) => {
                assert!(!backend.command.is_empty());
            }
            Err(RipError::NotAvailable) => {}
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }

    #[test]
    fn test_from_config_or_detect() {
        // With explicit command
        let result = MakemkvBackend::from_config_or_detect(Some("makemkvcon"));
        assert!(result.is_ok());

        // Without command, auto-detect (will fail in CI)
        let result = MakemkvBackend::from_config_or_detect(None);
        // OK if it fails — CI won't have it
        if let Err(e) = result {
            assert!(matches!(e, RipError::NotAvailable));
        }
    }
}
