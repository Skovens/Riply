use std::path::Path;
use std::process::Command;

use riply_core::ContentType;
use serde::Deserialize;
use tracing::{info, instrument, warn};

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    format: Option<FfprobeFormat>,
    streams: Option<Vec<FfprobeStream>>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    width: Option<u32>,
    height: Option<u32>,
    codec_name: Option<String>,
    codec_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FfprobeMetadata {
    pub duration_secs: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub codec: Option<String>,
    pub content_type: Option<ContentType>,
}

#[instrument]
pub fn extract_metadata(path: &Path) -> Result<FfprobeMetadata, anyhow::Error> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            path.to_string_lossy().as_ref(),
        ])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "ffprobe failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let probe: FfprobeOutput = serde_json::from_slice(&output.stdout)?;

    let duration_secs = probe
        .format
        .as_ref()
        .and_then(|f| f.duration.as_ref())
        .and_then(|d| d.parse::<f64>().ok());

    let video_stream = probe.streams.as_ref().and_then(|s| {
        s.iter()
            .find(|st| st.codec_type.as_deref() == Some("video"))
    });

    let width = video_stream.and_then(|s| s.width);
    let height = video_stream.and_then(|s| s.height);
    let codec = video_stream.and_then(|s| s.codec_name.clone());

    let content_type = match duration_secs {
        Some(d) if d > 3600.0 => Some(ContentType::Main),
        Some(_) => Some(ContentType::Extra),
        None => None,
    };

    Ok(FfprobeMetadata {
        duration_secs,
        width,
        height,
        codec,
        content_type,
    })
}

/// Video file extensions that ffprobe can inspect.
const VIDEO_EXTENSIONS: &[&str] = &["mkv", "mp4", "avi", "mov", "wmv", "flv", "webm"];

/// Find all video files in a directory (non-recursive).
pub fn find_video_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut videos = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                        videos.push(path);
                    }
                }
            }
        }
    }
    videos.sort_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0));
    videos
}

/// Check if a video file's duration matches the expected duration from TMDb.
///
/// Uses a tolerance of ±5 seconds or ±1% (whichever is larger) to account for
/// minor encoding differences between the disc source and the ripped file.
///
/// Returns `Ok(())` if the duration matches, `Err` with a description if it
/// differs significantly. Returns `Ok(None)` if either duration is unavailable.
pub fn verify_episode_duration(
    video_path: &Path,
    expected_duration_secs: f64,
) -> Result<Option<()>, String> {
    let metadata = extract_metadata(video_path).map_err(|e| e.to_string())?;

    let actual_secs = match metadata.duration_secs {
        Some(d) => d,
        None => {
            warn!(
                path = %video_path.display(),
                "Could not determine actual duration from ffprobe"
            );
            return Ok(None);
        }
    };

    if expected_duration_secs <= 0.0 {
        return Ok(None);
    }

    let diff = (actual_secs - expected_duration_secs).abs();
    // Tolerance: ±5 seconds or ±1%, whichever is larger
    let tolerance = 5.0_f64.max(expected_duration_secs * 0.01);

    if diff > tolerance {
        let pct = (diff / expected_duration_secs) * 100.0;
        let msg = format!(
            "Episode duration mismatch: actual {:.1}s vs expected {:.1}s (diff: {:.1}s, {:.1}%)",
            actual_secs, expected_duration_secs, diff, pct
        );
        warn!(
            path = %video_path.display(),
            actual_secs = actual_secs,
            expected_secs = expected_duration_secs,
            diff_secs = diff,
            pct_diff = pct,
            "{}",
            msg
        );
        return Err(msg);
    }

    info!(
        path = %video_path.display(),
        actual_secs = actual_secs,
        expected_secs = expected_duration_secs,
        "Episode duration verified"
    );
    Ok(Some(()))
}
