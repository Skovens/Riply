use std::path::Path;
use std::process::Command;

use riply_core::ContentType;
use serde::Deserialize;
use tracing::instrument;

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
