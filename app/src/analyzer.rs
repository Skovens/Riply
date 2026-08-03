use regex::Regex;
use riply_core::{MediaType, MetadataSearcher};
use std::sync::LazyLock;
use tracing::info;

use crate::ffprobe;

static SXXEXX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)S(\d{1,2})E(\d{1,3})").unwrap());

static SXX_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)S(\d{1,2})(?:\s|$)").unwrap());

static NXNN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(\d{1,2})x(\d{1,3})").unwrap());

static YEAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(19|20)\d{2}\b").unwrap());

static EPISODE_PATTERN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)S\d{1,2}E\d{1,3}(?:E\d{1,3})*").unwrap());

static NXNN_PATTERN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\d{1,2}x\d{1,3}").unwrap());

static SEASON_PATTERN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)S\d{1,2}(?:\s|$)").unwrap());

/// Analyzer service - orchestrates media analysis operations
pub struct AnalyzerService {
    searcher: Option<Box<dyn MetadataSearcher>>,
}

impl AnalyzerService {
    pub fn new(searcher: Option<Box<dyn MetadataSearcher>>) -> Self {
        info!("Analyzer service initialized");
        AnalyzerService { searcher }
    }

    /// Analyze a file to identify media
    pub async fn analyze_file(
        &self,
        file: &riply_core::FileRecord,
    ) -> Result<riply_core::FileRecord, riply_core::MetadataError> {
        info!("Analyzing file: {}", file.filename);

        let mut record = file.clone();

        // Extract media metadata via ffprobe (skip gracefully if not available)
        if let Ok(metadata) = ffprobe::extract_metadata(&record.path) {
            record.duration = metadata.duration_secs.map(|d| d as i64);
            if let Some(ref ct) = metadata.content_type {
                record.content_type = ct.clone();
            }
        }

        // Parse filename for title, year, and TV episode info
        let parsed = parse_filename(&record.filename);

        // If filename has TV episode markers, set media type early
        if parsed.season_number.is_some() || parsed.episode_number.is_some() {
            record.media_type = MediaType::Tv;
            record.season_number = parsed.season_number;
            record.episode_number = parsed.episode_number;
        }

        // Use the series/movie title for search (without episode info)
        let search_title = parsed.title.clone();
        let search_year = parsed.year;

        // Try external metadata search if available
        if let Some(ref searcher) = self.searcher {
            match searcher
                .search(&search_title, search_year, &record.media_type)
                .await
            {
                Ok(results) => {
                    if let Some(best) = results.into_iter().next() {
                        info!(
                            "TMDb match: '{}' ({})",
                            best.title,
                            best.year.map_or("n/a".to_string(), |y| y.to_string())
                        );
                        record.suggested_title = Some(best.title);
                        record.suggested_year = best.year;
                        record.tmdb_id = best.tmdb_id;
                        if best.is_tv {
                            record.media_type = MediaType::Tv;
                            // Use series name from API if available, otherwise keep search title
                            if let Some(ref series) = best.series_name {
                                record.series_name = Some(series.clone());
                            } else {
                                record.series_name = Some(search_title.clone());
                            }
                            // Season/episode from filename take precedence
                            if record.season_number.is_none() {
                                record.season_number = best.season_number;
                            }
                            if record.episode_number.is_none() {
                                record.episode_number = best.episode_number;
                            }
                            record.episode_title = best.episode_title;
                        }
                        record.confidence = Some(0.95);
                        record.source = Some("tmdb".to_string());
                        record.update_status(riply_core::PipelineStatus::Matched);
                        return Ok(record);
                    }
                }
                Err(e) => {
                    info!(
                        "Metadata search failed, falling back to filename parsing: {}",
                        e
                    );
                }
            }
        }

        // Fallback: filename-only heuristic
        let mut confidence: f64 = 0.0;
        if parsed.title.len() > 3 {
            confidence += 0.5;
        }
        if parsed.year.is_some() {
            confidence += 0.3;
        }
        if record.filename.contains(".") {
            confidence += 0.2;
        }

        record.suggested_title = Some(parsed.title);
        record.suggested_year = parsed.year;

        // TV episode detected from filename → higher base confidence
        if record.media_type.is_tv() {
            confidence = confidence.max(0.85_f64);
        }

        record.confidence = Some(confidence);

        if confidence >= 0.8 {
            record.update_status(riply_core::PipelineStatus::Matched);
        } else {
            record.update_status(riply_core::PipelineStatus::NeedsReview);
        }

        Ok(record)
    }
}

/// Parsed filename components
struct ParsedFilename {
    title: String,
    year: Option<u32>,
    season_number: Option<u32>,
    episode_number: Option<u32>,
}

/// Parse a filename to extract title, year, and optional TV episode info.
///
/// Supports patterns:
/// - `Show.S01E05.mkv` → season=1, episode=5
/// - `Show.S01E05E06.mkv` → season=1, episode=5 (multi-episode, takes first)
/// - `Show.S01.mkv` → season=1 (season pack)
/// - `Show.1x05.mkv` → season=1, episode=5
fn parse_filename(filename: &str) -> ParsedFilename {
    // Remove extension (keep everything before the last dot)
    let name = filename
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(filename);

    // Normalize separators to spaces so word boundaries work
    let normalized = name.replace(['.', '_'], " ");

    let mut season_number = None;
    let mut episode_number = None;

    // Try SxxExx pattern (e.g., S01E05, S01E05E06)
    if let Some(caps) = SXXEXX_RE.captures(&normalized) {
        season_number = caps.get(1).and_then(|m| m.as_str().parse().ok());
        episode_number = caps.get(2).and_then(|m| m.as_str().parse().ok());
    }

    // Try Sxx pattern without episode (season pack)
    if season_number.is_none() {
        if let Some(caps) = SXX_RE.captures(&normalized) {
            season_number = caps.get(1).and_then(|m| m.as_str().parse().ok());
        }
    }

    // Try NxNN pattern (e.g., 1x05)
    if episode_number.is_none() {
        if let Some(caps) = NXNN_RE.captures(&normalized) {
            season_number = caps.get(1).and_then(|m| m.as_str().parse().ok());
            episode_number = caps.get(2).and_then(|m| m.as_str().parse().ok());
        }
    }

    // Extract year (4 digits, typically in parentheses or after a dot)
    let year = YEAR_RE
        .find(&normalized)
        .and_then(|m| m.as_str().parse().ok());

    // Clean up title: remove episode markers and year for cleaner search
    let mut title = normalized.clone();

    // Remove SxxExx and NxNN patterns from title
    title = EPISODE_PATTERN_RE.replace(&title, "").to_string();

    title = NXNN_PATTERN_RE.replace(&title, "").to_string();

    // Remove standalone Sxx season packs
    title = SEASON_PATTERN_RE.replace(&title, "").to_string();

    // Remove year from title for cleaner search
    if let Some(ref year_re) = YEAR_RE.find(&title) {
        title = title[..year_re.start()].to_string() + &title[year_re.end()..];
    }

    // Collapse multiple spaces and trim
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = title.trim().to_string();

    ParsedFilename {
        title,
        year,
        season_number,
        episode_number,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riply_core::PipelineStatus;

    #[test]
    fn test_parse_filename_with_year() {
        let parsed = parse_filename("Movie.2023.mkv");
        assert_eq!(parsed.title, "Movie");
        assert_eq!(parsed.year, Some(2023));
        assert_eq!(parsed.season_number, None);
        assert_eq!(parsed.episode_number, None);
    }

    #[test]
    fn test_parse_filename_without_year() {
        let parsed = parse_filename("Movie.mkv");
        assert_eq!(parsed.title, "Movie");
        assert_eq!(parsed.year, None);
    }

    #[test]
    fn test_parse_filename_with_underscore() {
        let parsed = parse_filename("Movie_2023.mkv");
        assert_eq!(parsed.title, "Movie");
        assert_eq!(parsed.year, Some(2023));
    }

    #[test]
    fn test_parse_tv_sxxexx() {
        let parsed = parse_filename("Breaking.Bad.S01E05.mkv");
        assert_eq!(parsed.title, "Breaking Bad");
        assert_eq!(parsed.season_number, Some(1));
        assert_eq!(parsed.episode_number, Some(5));
    }

    #[test]
    fn test_parse_tv_sxxexx_with_year() {
        let parsed = parse_filename("Breaking.Bad.S01E05.2008.mkv");
        assert_eq!(parsed.title, "Breaking Bad");
        assert_eq!(parsed.year, Some(2008));
        assert_eq!(parsed.season_number, Some(1));
        assert_eq!(parsed.episode_number, Some(5));
    }

    #[test]
    fn test_parse_tv_multi_episode() {
        let parsed = parse_filename("Breaking.Bad.S01E05E06.mkv");
        assert_eq!(parsed.title, "Breaking Bad");
        assert_eq!(parsed.season_number, Some(1));
        assert_eq!(parsed.episode_number, Some(5));
    }

    #[test]
    fn test_parse_tv_season_pack() {
        let parsed = parse_filename("Breaking.Bad.S02.mkv");
        assert_eq!(parsed.title, "Breaking Bad");
        assert_eq!(parsed.season_number, Some(2));
        assert_eq!(parsed.episode_number, None);
    }

    #[test]
    fn test_parse_tv_nxnn() {
        let parsed = parse_filename("Breaking.Bad.1x05.mkv");
        assert_eq!(parsed.title, "Breaking Bad");
        assert_eq!(parsed.season_number, Some(1));
        assert_eq!(parsed.episode_number, Some(5));
    }

    #[test]
    fn test_parse_tv_underscore_format() {
        let parsed = parse_filename("Breaking_Bad_S01E05.mkv");
        assert_eq!(parsed.title, "Breaking Bad");
        assert_eq!(parsed.season_number, Some(1));
        assert_eq!(parsed.episode_number, Some(5));
    }

    #[tokio::test]
    async fn test_analyze_file_with_year() {
        let file = riply_core::FileRecord::new(
            "test".to_string(),
            std::path::PathBuf::from("/test/movie.2023.mkv"),
            "movie.2023.mkv".to_string(),
            "hash".to_string(),
        );

        let analyzer = AnalyzerService::new(None);
        let result = analyzer.analyze_file(&file).await.unwrap();

        assert_eq!(result.suggested_title, Some("movie".to_string()));
        assert_eq!(result.suggested_year, Some(2023));
        assert_eq!(result.confidence, Some(1.0));
        assert_eq!(result.status, PipelineStatus::Matched);
    }

    #[tokio::test]
    async fn test_analyze_file_without_year() {
        let file = riply_core::FileRecord::new(
            "test".to_string(),
            std::path::PathBuf::from("/test/movie.mkv"),
            "movie.mkv".to_string(),
            "hash".to_string(),
        );

        let analyzer = AnalyzerService::new(None);
        let result = analyzer.analyze_file(&file).await.unwrap();

        assert_eq!(result.suggested_title, Some("movie".to_string()));
        assert_eq!(result.suggested_year, None);
        assert_eq!(result.confidence, Some(0.7));
        assert_eq!(result.status, PipelineStatus::NeedsReview);
    }

    #[tokio::test]
    async fn test_analyze_tv_episode_filename() {
        let file = riply_core::FileRecord::new(
            "test".to_string(),
            std::path::PathBuf::from("/test/Breaking.Bad.S01E05.mkv"),
            "Breaking.Bad.S01E05.mkv".to_string(),
            "hash".to_string(),
        );

        let analyzer = AnalyzerService::new(None);
        let result = analyzer.analyze_file(&file).await.unwrap();

        assert_eq!(result.media_type, MediaType::Tv);
        assert_eq!(result.season_number, Some(1));
        assert_eq!(result.episode_number, Some(5));
        assert_eq!(result.suggested_title, Some("Breaking Bad".to_string()));
        // TV episodes get high confidence from filename alone
        assert!(result.confidence.unwrap() >= 0.85);
        assert_eq!(result.status, PipelineStatus::Matched);
    }

    /// Test analyzer with a mock metadata searcher
    #[tokio::test]
    async fn test_analyze_file_with_metadata_searcher() {
        struct MockSearcher;

        impl riply_core::MetadataSearcher for MockSearcher {
            fn search_movies(
                &self,
                _query: &str,
                _year: Option<u32>,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<Vec<riply_core::MetadataMatch>, riply_core::MetadataError>,
                        > + Send
                        + '_,
                >,
            > {
                Box::pin(async move {
                    Ok(vec![riply_core::MetadataMatch {
                        title: "The Matrix".to_string(),
                        year: Some(1999),
                        tmdb_id: Some(603),
                        is_tv: false,
                        series_name: None,
                        season_number: None,
                        episode_number: None,
                        episode_title: None,
                    }])
                })
            }

            fn search_tv(
                &self,
                _query: &str,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<Vec<riply_core::MetadataMatch>, riply_core::MetadataError>,
                        > + Send
                        + '_,
                >,
            > {
                Box::pin(async move { Ok(vec![]) })
            }
        }

        let file = riply_core::FileRecord::new(
            "test".to_string(),
            std::path::PathBuf::from("/test/matrix.1999.mkv"),
            "matrix.1999.mkv".to_string(),
            "hash".to_string(),
        );

        let analyzer = AnalyzerService::new(Some(Box::new(MockSearcher)));
        let result = analyzer.analyze_file(&file).await.unwrap();

        assert_eq!(result.suggested_title, Some("The Matrix".to_string()));
        assert_eq!(result.suggested_year, Some(1999));
        assert_eq!(result.tmdb_id, Some(603));
        assert_eq!(result.confidence, Some(0.95));
        assert_eq!(result.source, Some("tmdb".to_string()));
        assert_eq!(result.status, PipelineStatus::Matched);
    }

    /// Test analyzer with TV metadata searcher
    #[tokio::test]
    async fn test_analyze_tv_with_metadata_searcher() {
        struct MockTvSearcher;

        impl riply_core::MetadataSearcher for MockTvSearcher {
            fn search_movies(
                &self,
                _query: &str,
                _year: Option<u32>,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<Vec<riply_core::MetadataMatch>, riply_core::MetadataError>,
                        > + Send
                        + '_,
                >,
            > {
                Box::pin(async move { Ok(vec![]) })
            }

            fn search_tv(
                &self,
                _query: &str,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<Vec<riply_core::MetadataMatch>, riply_core::MetadataError>,
                        > + Send
                        + '_,
                >,
            > {
                Box::pin(async move {
                    Ok(vec![riply_core::MetadataMatch {
                        title: "Breaking Bad".to_string(),
                        year: Some(2008),
                        tmdb_id: Some(1396),
                        is_tv: true,
                        series_name: Some("Breaking Bad".to_string()),
                        season_number: Some(1),
                        episode_number: Some(5),
                        episode_title: Some("Gray Matter".to_string()),
                    }])
                })
            }
        }

        let file = riply_core::FileRecord::new(
            "test".to_string(),
            std::path::PathBuf::from("/test/Breaking.Bad.S01E05.mkv"),
            "Breaking.Bad.S01E05.mkv".to_string(),
            "hash".to_string(),
        );

        let analyzer = AnalyzerService::new(Some(Box::new(MockTvSearcher)));
        let result = analyzer.analyze_file(&file).await.unwrap();

        assert_eq!(result.media_type, MediaType::Tv);
        assert_eq!(result.series_name, Some("Breaking Bad".to_string()));
        assert_eq!(result.season_number, Some(1));
        assert_eq!(result.episode_number, Some(5));
        assert_eq!(result.episode_title, Some("Gray Matter".to_string()));
        assert_eq!(result.suggested_title, Some("Breaking Bad".to_string()));
        assert_eq!(result.tmdb_id, Some(1396));
        assert_eq!(result.confidence, Some(0.95));
        assert_eq!(result.source, Some("tmdb".to_string()));
        assert_eq!(result.status, PipelineStatus::Matched);
    }
}
