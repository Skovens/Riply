use std::path::Path;
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
                    info!("Found makemkvcon at: {}", path);
                    return Ok(Self::new(vec![path]));
                }
            }
        }

        // Try common paths
        for path in &["/usr/bin/makemkvcon", "/usr/local/bin/makemkvcon"] {
            if Path::new(path).exists() {
                info!("Found makemkvcon at: {}", path);
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
                info!("Using configured ripping command: {}", cmd);
                Ok(Self::from_config(cmd))
            }
            _ => Self::auto_detect(),
        }
    }

    /// Run `makemkvcon` with the given args and return stdout.
    fn run_command(&self, args: &[&str]) -> Result<String, RipError> {
        debug!("Running: {} {}", self.command.join(" "), args.join(" "));

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
        info!("Detecting drives via {}", self.command.join(" "));

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

                    drives.push(DriveInfo {
                        index: parsed.index,
                        name: parsed.drive_name,
                        disc_name: parsed.disc_name,
                        device_path: parsed.device_path,
                        state,
                    });
                }
            }
        }

        debug!("Found {} drives", drives.len());
        Ok(drives)
    }

    fn start_rip(&self, drive: usize, dest: &str) -> Result<RipHandle, RipError> {
        let dest_path = Path::new(dest);
        if !dest_path.exists() {
            std::fs::create_dir_all(dest_path)?;
        }

        info!("Starting rip: drive {} → {}", drive, dest);

        let child = tokio::process::Command::new(&self.command[0])
            .args(&self.command[1..])
            .args([
                "-r",
                "--cache=1024",
                "--noscan",
                "mkv",
                &format!("disc:{}", drive),
                "all",
                dest,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let session = MakemkvSession::new(child);
        Ok(RipHandle::new(Box::new(session)))
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
