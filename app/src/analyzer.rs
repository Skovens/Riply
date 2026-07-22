use regex::Regex;
use tracing::info;

/// Analyzer service - orchestrates media analysis operations
pub struct AnalyzerService;

impl Default for AnalyzerService {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalyzerService {
    /// Create a new analyzer service
    pub fn new() -> Self {
        info!("Analyzer service initialized");
        AnalyzerService
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

        // Calculate confidence based on how much info we found
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

        // Update status based on confidence
        if confidence >= 0.8 {
            record.update_status(riply_core::PipelineStatus::Matched);
        } else {
            record.update_status(riply_core::PipelineStatus::NeedsReview);
        }

        Ok(record)
    }

    /// Search for media metadata
    pub async fn search_metadata(
        &self,
        _query: &str,
        _media_type: &riply_core::MediaType,
    ) -> Result<Vec<riply_core::FileRecord>, anyhow::Error> {
        info!("Searching metadata for: {} ({})", _query, _media_type);
        // TODO: Implement TMDb/OMDb search in Phase 2
        Ok(vec![])
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

        let analyzer = AnalyzerService::new();
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

        let analyzer = AnalyzerService::new();
        let result = analyzer.analyze_file(&file).await.unwrap();

        assert_eq!(result.suggested_title, Some("movie".to_string()));
        assert_eq!(result.suggested_year, None);
        assert_eq!(result.confidence, Some(0.7));
        assert_eq!(result.status, PipelineStatus::NeedsReview);
    }
}
