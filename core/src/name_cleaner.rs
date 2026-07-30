use unidecode::unidecode;

/// Cleans and normalizes media titles for consistent matching and display.
pub struct NameCleaner;

impl NameCleaner {
    /// Clean a raw name for display: normalize unicode, title-case, collapse whitespace.
    ///
    /// Replaces underscores and other machine separators with spaces.
    ///
    /// ```
    /// use riply_core::NameCleaner;
    ///
    /// assert_eq!(NameCleaner::clean_name("the_dark_knight"), "The Dark Knight");
    /// assert_eq!(NameCleaner::clean_name("INCEPTION_2010"), "Inception 2010");
    /// assert_eq!(NameCleaner::clean_name("MÖTLEY_CRÜE"), "Motley Crue");
    /// ```
    pub fn clean_name(name: &str) -> String {
        let separated = name.replace('_', " ");
        let ascii = unidecode(&separated);
        ascii
            .split_whitespace()
            .map(title_case_word)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Clean a name for search queries: same as `clean_name` but strips a trailing
    /// 4-digit year so it doesn't interfere with API searches.
    ///
    /// Only strips the year if it appears as the **last** word, to avoid mangling
    /// titles like "2001: A Space Odyssey".
    ///
    /// ```
    /// use riply_core::NameCleaner;
    ///
    /// assert_eq!(NameCleaner::clean_name_for_search("Inception 2010"), "Inception");
    /// assert_eq!(NameCleaner::clean_name_for_search("The Dark Knight"), "The Dark Knight");
    /// assert_eq!(NameCleaner::clean_name_for_search("2001 A Space Odyssey"), "2001 A Space Odyssey");
    /// ```
    pub fn clean_name_for_search(name: &str) -> String {
        let cleaned = Self::clean_name(name);
        let mut words: Vec<&str> = cleaned.split_whitespace().collect();
        if words.len() > 1 && is_year_word(words.last().unwrap()) {
            words.pop();
        }
        words.join(" ")
    }
}

fn title_case_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let rest = chars.as_str().to_lowercase();
            first.to_uppercase().to_string() + &rest
        }
    }
}

fn is_year_word(word: &str) -> bool {
    if word.len() != 4 {
        return false;
    }
    let year: u16 = match word.parse() {
        Ok(y) => y,
        Err(_) => return false,
    };
    (1900..=2100).contains(&year)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_name_basic() {
        assert_eq!(NameCleaner::clean_name("inception"), "Inception");
        assert_eq!(
            NameCleaner::clean_name("THE_DARK_KNIGHT"),
            "The Dark Knight"
        );
        assert_eq!(NameCleaner::clean_name("braveheart"), "Braveheart");
    }

    #[test]
    fn test_clean_name_with_year() {
        assert_eq!(NameCleaner::clean_name("INCEPTION_2010"), "Inception 2010");
        assert_eq!(
            NameCleaner::clean_name("The Dark Knight 2008"),
            "The Dark Knight 2008"
        );
    }

    #[test]
    fn test_clean_name_unicode() {
        assert_eq!(NameCleaner::clean_name("MÖTLEY_CRÜE"), "Motley Crue");
        assert_eq!(NameCleaner::clean_name("Café"), "Cafe");
    }

    #[test]
    fn test_clean_name_whitespace_collapse() {
        assert_eq!(
            NameCleaner::clean_name("  The   Dark   Knight  "),
            "The Dark Knight"
        );
    }

    #[test]
    fn test_clean_name_mixed_case() {
        assert_eq!(
            NameCleaner::clean_name("tHe DaRk KnIgHt"),
            "The Dark Knight"
        );
    }

    #[test]
    fn test_clean_name_empty() {
        assert_eq!(NameCleaner::clean_name(""), "");
    }

    #[test]
    fn test_clean_name_for_search_basic() {
        assert_eq!(
            NameCleaner::clean_name_for_search("Inception 2010"),
            "Inception"
        );
        assert_eq!(
            NameCleaner::clean_name_for_search("The Dark Knight"),
            "The Dark Knight"
        );
    }

    #[test]
    fn test_clean_name_for_search_preserves_non_trailing_year() {
        assert_eq!(
            NameCleaner::clean_name_for_search("2001 A Space Odyssey"),
            "2001 A Space Odyssey"
        );
    }

    #[test]
    fn test_clean_name_for_search_trailing_year_only() {
        assert_eq!(
            NameCleaner::clean_name_for_search("Inception 2010"),
            "Inception"
        );
        assert_eq!(
            NameCleaner::clean_name_for_search("The Dark Knight 2008"),
            "The Dark Knight"
        );
    }

    #[test]
    fn test_clean_name_for_search_no_year() {
        assert_eq!(
            NameCleaner::clean_name_for_search("The Matrix"),
            "The Matrix"
        );
    }
}
