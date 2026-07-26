use regex::Regex;
use riply_core::{MediaType, MetadataSearcher};
use tracing::info;

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
    ) -> Result<riply_core::FileRecord, anyhow::Error> {
        info!("Analyzing file: {}", file.filename);

        let mut record = file.clone();

        // Parse filename for title and year hints
        let (title, year) = parse_filename(&record.filename);

        // Try external metadata search if available
        if let Some(ref searcher) = self.searcher {
            match searcher.search(&title, year, &record.media_type).await {
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
        let mut confidence = 0.0;
        if title.len() > 3 {
            confidence += 0.5;
        }
        if year.is_some() {
            confidence += 0.3;
        }
        if record.filename.contains(".") {
            confidence += 0.2;
        }

        record.suggested_title = Some(title);
        record.suggested_year = year;
        record.confidence = Some(confidence);

        if confidence >= 0.8 {
            record.update_status(riply_core::PipelineStatus::Matched);
        } else {
            record.update_status(riply_core::PipelineStatus::NeedsReview);
        }

        Ok(record)
    }
}

/// Parse a filename to extract title and year
fn parse_filename(filename: &str) -> (String, Option<u32>) {
    // Remove extension (keep everything before the last dot)
    let name = filename
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(filename);

    // Normalize separators to spaces so word boundaries work
    let normalized = name.replace(['.', '_'], " ");

    // Try to extract year (4 digits, typically in parentheses or after a dot)
    let year_regex = Regex::new(r"\b(19|20)\d{2}\b").unwrap();
    let year = year_regex
        .find(&normalized)
        .and_then(|m| m.as_str().parse().ok());

    // Clean up title
    let title = normalized.trim().to_string();

    (title, year)
}

#[cfg(test)]
mod tests {
    use super::*;
    use riply_core::PipelineStatus;

    #[test]
    fn test_parse_filename_with_year() {
        let (title, year) = parse_filename("Movie.2023.mkv");
        assert_eq!(title, "Movie 2023");
        assert_eq!(year, Some(2023));
    }

    #[test]
    fn test_parse_filename_without_year() {
        let (title, year) = parse_filename("Movie.mkv");
        assert_eq!(title, "Movie");
        assert_eq!(year, None);
    }

    #[test]
    fn test_parse_filename_with_underscore() {
        let (title, year) = parse_filename("Movie_2023.mkv");
        assert_eq!(title, "Movie 2023");
        assert_eq!(year, Some(2023));
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

        assert_eq!(result.suggested_title, Some("movie 2023".to_string()));
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
                            Output = Result<Vec<riply_core::MetadataMatch>, anyhow::Error>,
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
                    }])
                })
            }

            fn search_tv(
                &self,
                _query: &str,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<Vec<riply_core::MetadataMatch>, anyhow::Error>,
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
}
