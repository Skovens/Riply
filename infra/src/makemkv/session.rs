use std::future::Future;
use std::pin::Pin;

use riply_core::{RipError, RipEvent, RipSession};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::mpsc;
use tracing::{debug, info};

use super::parser;

/// MakeMKV session wrapping a child process and event channel.
pub struct MakemkvSession {
    child: Option<Child>,
    rx: mpsc::UnboundedReceiver<RipEvent>,
}

impl MakemkvSession {
    /// Create a new session from a spawned child process.
    /// Starts a reader task that parses stdout lines into events.
    pub fn new(mut child: Child) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Reader task for stdout
        if let Some(stdout) = stdout {
            let tx = tx.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();

                while let Ok(Some(line)) = lines.next_line().await {
                    debug!(line = %line, "makemkvcon stdout");

                    if let Some(prog) = parser::parse_progress_line(&line) {
                        let _ = tx.send(RipEvent::Progress {
                            current: prog.current,
                            total: prog.total,
                            max: prog.max,
                        });
                    } else if parser::is_completion_line(&line) {
                        let _ = tx.send(RipEvent::Complete);
                    } else if parser::is_error_message(&line) {
                        if let Some(msg) = parser::parse_message_line(&line) {
                            let _ = tx.send(RipEvent::Error(msg.message));
                        }
                    } else if let Some(msg) = parser::parse_message_line(&line) {
                        let _ = tx.send(RipEvent::Message(msg.message));
                    }
                }

                debug!("Stdout reader finished");
            });
        }

        // Reader task for stderr (log messages)
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();

                while let Ok(Some(line)) = lines.next_line().await {
                    debug!(line = %line, "makemkvcon stderr");
                }
            });
        }

        Self {
            child: Some(child),
            rx,
        }
    }
}

impl RipSession for MakemkvSession {
    fn next_event(&mut self) -> Pin<Box<dyn Future<Output = Option<RipEvent>> + Send + '_>> {
        Box::pin(async move { self.rx.recv().await })
    }

    fn cancel(&mut self) -> Pin<Box<dyn Future<Output = Result<(), RipError>> + Send + '_>> {
        Box::pin(async move {
            if let Some(ref mut child) = self.child {
                info!("Killing makemkvcon process");
                child.kill().await.map_err(RipError::Io)?;
                self.child = None;
            }
            Ok(())
        })
    }
}

impl Drop for MakemkvSession {
    fn drop(&mut self) {
        // kill_on_drop is set when spawning, so child is killed automatically.
        // This is a safety fallback.
        if let Some(ref mut child) = self.child {
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_cancel() {
        // Spawn a long-running process and cancel it
        let child = tokio::process::Command::new("sleep")
            .arg("100")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        let mut session = MakemkvSession::new(child);
        let result = session.cancel().await;
        assert!(result.is_ok());
        assert!(session.child.is_none());
    }
}
