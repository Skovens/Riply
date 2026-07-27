pub mod backend;
pub mod dryrun;
pub mod parser;
pub mod session;
pub mod setup;

use std::process::Command;

pub use backend::MakemkvBackend;
pub use dryrun::DryRunBackend;
pub use session::MakemkvSession;
pub use setup::{check_installation, install_makemkv, InstallationStatus};

/// Eject a disc from the given device path (e.g. `/dev/sr0`).
///
/// Uses the Linux `eject` command. Returns `Ok(())` if the disc was ejected
/// or the drive was already empty. Returns an error if the device doesn't
/// exist or eject fails for another reason.
pub fn eject_disc(device_path: &str) -> Result<(), anyhow::Error> {
    match Command::new("eject").arg(device_path).output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No medium found") {
                Ok(())
            } else if stderr.contains("not found") || stderr.contains("No such") {
                Err(anyhow::anyhow!(
                    "Device {} not found. Check that the drive index is correct.",
                    device_path
                ))
            } else {
                Err(anyhow::anyhow!("Eject failed: {}", stderr.trim()))
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Err(anyhow::anyhow!(
                    "'eject' command not found. Install with: sudo apt-get install eject"
                ))
            } else {
                Err(e.into())
            }
        }
    }
}
