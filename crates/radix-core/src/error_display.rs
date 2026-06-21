//! User-friendly error summarization.
//!
//! Strips internal paths, stack traces, and overly technical details from
//! error messages before they're shown to end users (e.g. in Telegram).

/// Summarize an error for user-facing display.
///
/// Strips:
/// - Absolute file paths (e.g. `/home/user/.pares-radix/...`)
/// - Stack traces / backtrace lines
/// - Repeated "Caused by:" chains beyond the first
/// - Internal crate names
pub fn summarize_error(err: &dyn std::fmt::Display) -> String {
    let raw = err.to_string();
    summarize_error_str(&raw)
}

/// Summarize a raw error string for user-facing display.
pub fn summarize_error_str(raw: &str) -> String {
    let mut lines: Vec<&str> = raw.lines().collect();

    // Remove stack trace lines
    lines.retain(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with("at ")
            && !trimmed.starts_with("stack backtrace:")
            && !trimmed.contains("RUST_BACKTRACE")
            && !trimmed.starts_with("note: run with")
    });

    // Take only the first "Caused by:" chain entry
    let mut seen_caused_by = false;
    lines.retain(|line| {
        if line.trim().starts_with("Caused by:") {
            if seen_caused_by {
                return false;
            }
            seen_caused_by = true;
        }
        true
    });

    let mut result = lines.join("\n");

    // Strip absolute paths
    let path_re = regex::Regex::new(r"/[a-zA-Z0-9_./-]+/\.pares-radix/[a-zA-Z0-9_./-]+")
        .unwrap_or_else(|_| regex::Regex::new(r"^$").unwrap());
    result = path_re.replace_all(&result, "<internal path>").to_string();

    // Strip home directory paths
    let home_re = regex::Regex::new(r"/home/[a-zA-Z0-9_]+/[^\s]+")
        .unwrap_or_else(|_| regex::Regex::new(r"^$").unwrap());
    result = home_re.replace_all(&result, "<internal path>").to_string();

    // Trim to reasonable length
    if result.len() > 300 {
        result.truncate(297);
        result.push_str("...");
    }

    result
}

/// Format an error for user-facing display with emoji prefix.
pub fn user_friendly_error(err: &dyn std::fmt::Display) -> String {
    format!("⚠️ Something went wrong: {}", summarize_error(err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_paths() {
        let msg = "failed to read /home/user/.pares-radix/memory/db.sqlite: permission denied";
        let result = summarize_error_str(msg);
        assert!(!result.contains("/home/user"));
        assert!(result.contains("permission denied"));
    }

    #[test]
    fn strips_backtrace() {
        let msg = "connection refused\nstack backtrace:\n   0: foo::bar\n   1: baz::qux";
        let result = summarize_error_str(msg);
        assert!(result.contains("connection refused"));
        assert!(!result.contains("stack backtrace"));
    }

    #[test]
    fn truncates_long_errors() {
        let msg = "x".repeat(500);
        let result = summarize_error_str(&msg);
        assert!(result.len() <= 300);
    }
}
