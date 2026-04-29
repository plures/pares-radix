//! PII guard — redacts sensitive patterns before sending to external models.

use serde::{Deserialize, Serialize};

/// A pattern-based PII redactor.
pub struct PiiGuard {
    patterns: Vec<(regex::Regex, &'static str)>,
}

/// Summary of redactions performed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RedactionReport {
    pub redactions: Vec<String>,
    pub count: usize,
}

impl Default for PiiGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl PiiGuard {
    /// Create a new PII guard with default patterns.
    pub fn new() -> Self {
        let patterns: Vec<(regex::Regex, &'static str)> = vec![
            // GitHub PATs (classic and fine-grained)
            (regex::Regex::new(r"ghp_[A-Za-z0-9]{36}").unwrap(), "[GITHUB_PAT]"),
            (regex::Regex::new(r"github_pat_[A-Za-z0-9_]{82}").unwrap(), "[GITHUB_PAT]"),
            // OpenAI keys
            (regex::Regex::new(r"sk-[A-Za-z0-9]{20}T3BlbkFJ[A-Za-z0-9]{20}").unwrap(), "[OPENAI_KEY]"),
            (regex::Regex::new(r"sk-proj-[A-Za-z0-9\-_]{40,}").unwrap(), "[OPENAI_KEY]"),
            // AWS keys
            (regex::Regex::new(r"AKIA[A-Z0-9]{16}").unwrap(), "[AWS_KEY]"),
            // Azure/Microsoft tokens
            (regex::Regex::new(r"eyJ[A-Za-z0-9\-_]{50,}\.[A-Za-z0-9\-_]{50,}\.[A-Za-z0-9\-_]{20,}").unwrap(), "[JWT_TOKEN]"),
            // Private keys
            (regex::Regex::new(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----").unwrap(), "[PRIVATE_KEY_HEADER]"),
            // Generic long hex secrets (64+ chars, likely keys)
            (regex::Regex::new(r"\b[0-9a-f]{64,}\b").unwrap(), "[HEX_SECRET]"),
        ];
        Self { patterns }
    }

    /// Redact PII from text. Returns (redacted_text, report).
    pub fn redact(&self, text: &str) -> (String, RedactionReport) {
        let mut redacted = text.to_string();
        let mut report = RedactionReport::default();

        for (pattern, replacement) in &self.patterns {
            if pattern.is_match(&redacted) {
                let match_count = pattern.find_iter(&redacted).count();
                report.redactions.push(format!(
                    "{}x {}",
                    match_count, replacement
                ));
                report.count += match_count;
                redacted = pattern.replace_all(&redacted, *replacement).into_owned();
            }
        }

        (redacted, report)
    }

    /// Check if text contains any sensitive patterns (without modifying).
    pub fn contains_sensitive(&self, text: &str) -> bool {
        self.patterns.iter().any(|(p, _)| p.is_match(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_pat_redaction() {
        let guard = PiiGuard::new();
        let input = "My token is ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let (redacted, report) = guard.redact(input);
        assert!(redacted.contains("[GITHUB_PAT]"));
        assert!(!redacted.contains("ghp_"));
        assert_eq!(report.count, 1);
    }

    #[test]
    fn test_private_key_header() {
        let guard = PiiGuard::new();
        let input = "-----BEGIN RSA PRIVATE KEY-----\nMIIEo...";
        let (redacted, _) = guard.redact(input);
        assert!(redacted.contains("[PRIVATE_KEY_HEADER]"));
    }

    #[test]
    fn test_no_false_positives_on_normal_text() {
        let guard = PiiGuard::new();
        let input = "Hello world, please list the files in /tmp";
        let (redacted, report) = guard.redact(input);
        assert_eq!(redacted, input);
        assert_eq!(report.count, 0);
    }

    #[test]
    fn test_aws_key() {
        let guard = PiiGuard::new();
        let input = "AWS key: AKIAIOSFODNN7EXAMPLE";
        let (redacted, _) = guard.redact(input);
        assert!(redacted.contains("[AWS_KEY]"));
    }

    #[test]
    fn test_contains_sensitive() {
        let guard = PiiGuard::new();
        assert!(guard.contains_sensitive("ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij"));
        assert!(!guard.contains_sensitive("hello world"));
    }
}
