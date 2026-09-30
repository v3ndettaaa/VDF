//! Search query model.
//!
//! Unicode normalization beyond case folding (NFKC, Arabic presentation
//! forms, ZWNJ/tatweel) is deliberately deferred to M4 with the indexer —
//! `normalize_for_search` documents that contract instead of silently
//! half-implementing it.

use vdf_core::{VdfError, VdfResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub text: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

impl SearchQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            case_sensitive: false,
            whole_word: false,
            regex: false,
        }
    }

    pub fn case_sensitive(mut self, v: bool) -> Self {
        self.case_sensitive = v;
        self
    }

    pub fn whole_word(mut self, v: bool) -> Self {
        self.whole_word = v;
        self
    }

    pub fn regex(mut self, v: bool) -> Self {
        self.regex = v;
        self
    }

    /// Whether `haystack` contains a match for this query.
    ///
    /// Case-insensitivity uses full Unicode case folding via
    /// `str::to_lowercase`; word boundaries treat alphanumeric ASCII plus
    /// `_` as word characters (Unicode-aware boundaries arrive with the M4
    /// indexer, which needs UAX#29 for RTL scripts anyway).
    pub fn matches(&self, haystack: &str) -> VdfResult<bool> {
        if self.text.is_empty() {
            return Ok(false);
        }
        let (needle, hay) = if self.case_sensitive {
            (self.text.clone(), haystack.to_string())
        } else {
            (
                normalize_for_search(&self.text),
                normalize_for_search(haystack),
            )
        };

        if self.regex {
            let re = regex::Regex::new(&needle)
                .map_err(|e| VdfError::Search(format!("invalid regex: {e}")))?;
            return Ok(re.is_match(&hay));
        }

        if !self.whole_word {
            return Ok(hay.contains(&needle));
        }

        // whole-word substring scan
        let hay_bytes = hay.as_bytes();
        let needle_bytes = needle.as_bytes();
        if needle_bytes.is_empty() || needle_bytes.len() > hay_bytes.len() {
            return Ok(false);
        }
        let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        let mut start = 0;
        while let Some(rel) = hay[start..].find(&needle) {
            let at = start + rel;
            let end = at + needle.len();
            let left_ok = at == 0 || !is_word(hay_bytes[at - 1]);
            let right_ok = end == hay_bytes.len() || !is_word(hay_bytes[end]);
            if left_ok && right_ok {
                return Ok(true);
            }
            start = at + 1;
            if start + needle.len() > hay_bytes.len() {
                break;
            }
        }
        Ok(false)
    }
}

/// Case folding + whitespace trimming for search normalization.
///
/// NOT YET: NFKC normalization and Arabic/Persian-specific folding
/// (presentation forms, tatweel, ZWNJ) — M4, together with the indexer,
/// because offsets must stay consistent between index and highlight rects.
pub fn normalize_for_search(s: &str) -> String {
    s.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_case_sensitivity() {
        let q = SearchQuery::new("Hello");
        assert!(q.matches("say hello world").unwrap());
        assert!(q.matches("HELLO THERE").unwrap());
        assert!(!q.matches("hell").unwrap());

        let cs = SearchQuery::new("Hello").case_sensitive(true);
        assert!(cs.matches("Well Hello there").unwrap());
        assert!(!cs.matches("well hello there").unwrap());
    }

    #[test]
    fn whole_word_boundaries() {
        let q = SearchQuery::new("cat").whole_word(true);
        assert!(q.matches("a cat sat").unwrap());
        assert!(!q.matches("concatenate").unwrap());
        assert!(!q.matches("category").unwrap());
        assert!(q.matches("cat.").unwrap());
        assert!(q.matches("cat's bowl").unwrap(), "apostrophe is a boundary");
    }

    #[test]
    fn regex_mode() {
        let q = SearchQuery::new("colou?r").regex(true);
        assert!(q.matches("color").unwrap());
        assert!(q.matches("my favourite colour is").unwrap());
        assert!(!q.matches("colr").unwrap());

        // case-insensitive regex
        assert!(q.matches("COLOUR").unwrap());

        let bad = SearchQuery::new("[unclosed").regex(true);
        assert!(
            bad.matches("anything").is_err(),
            "invalid regex must error, not match"
        );
    }

    #[test]
    fn empty_needle_matches_nothing() {
        let q = SearchQuery::new("");
        assert!(!q.matches("anything").unwrap());
    }

    #[test]
    fn persian_caseless_search_still_works() {
        // Persian has no case; normalization must not break matching
        let q = SearchQuery::new("کتاب");
        assert!(q.matches("این یک کتاب است").unwrap());
        assert!(!q.matches("این یک قلم است").unwrap());
    }
}
