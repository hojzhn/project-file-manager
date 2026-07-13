use std::sync::LazyLock;

use regex::Regex;

static DATE_PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}[-\s]*").unwrap());

pub fn strip_date_prefix(name: &str) -> std::borrow::Cow<'_, str> {
    DATE_PREFIX.replace(name, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_leading_date() {
        assert_eq!(strip_date_prefix("2026-07-13-My Project"), "My Project");
        assert_eq!(strip_date_prefix("2026-07-13 My Project"), "My Project");
        assert_eq!(strip_date_prefix("2026-07-13"), "");
    }

    #[test]
    fn leaves_non_date_names_untouched() {
        assert_eq!(strip_date_prefix("My Project"), "My Project");
        assert_eq!(strip_date_prefix("2026-7-13-My Project"), "2026-7-13-My Project");
    }
}
