//! Bounded text matching shared by readers, predicates and completion.

use std::ops::Range;

use regex::{Regex, RegexBuilder};

use crate::foundation::error::{Error, Result};

#[derive(Clone, Debug)]
pub struct Matcher(Regex);

impl Matcher {
    pub fn new(pattern: &str, mode: &str, ignore_case: bool) -> Result<Self> {
        if pattern.len() > 16_384 {
            return Err(Error::usage("search pattern exceeds 16384 bytes"));
        }
        let literal = regex::escape(pattern);
        let expression = match mode {
            "literal" | "contains" => literal,
            "exact" => format!("\\A(?:{literal})\\z"),
            "prefix" => format!("\\A(?:{literal})"),
            "regex" => pattern.to_string(),
            _ => return Err(Error::usage(format!("unknown match mode `{mode}`"))),
        };
        RegexBuilder::new(&expression)
            .case_insensitive(ignore_case)
            .size_limit(1_048_576)
            .dfa_size_limit(1_048_576)
            .build()
            .map(Self)
            .map_err(|error| Error::usage(format!("invalid search pattern: {error}")))
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.0.is_match(text)
    }

    pub fn ranges(&self, text: &str) -> Vec<Range<usize>> {
        self.0
            .find_iter(text)
            .filter(|hit| !hit.is_empty())
            .map(|hit| hit.range())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_regex_and_case_have_one_bounded_contract() {
        assert!(
            Matcher::new("a.b", "literal", true)
                .unwrap()
                .is_match("A.B")
        );
        assert!(
            !Matcher::new("a.b", "literal", true)
                .unwrap()
                .is_match("axb")
        );
        assert_eq!(
            Matcher::new("a.b", "regex", true)
                .unwrap()
                .ranges("axb A.B")
                .len(),
            2
        );
        assert!(
            !Matcher::new("Clerk", "exact", false)
                .unwrap()
                .is_match("clerk")
        );
        assert!(
            Matcher::new("eng-04", "prefix", true)
                .unwrap()
                .is_match("ENG-042")
        );
        assert!(Matcher::new("[", "regex", true).is_err());
        assert!(Matcher::new(&"a".repeat(16_385), "literal", true).is_err());
        assert!(Matcher::new("(?=a)", "regex", false).is_err());
    }
}
