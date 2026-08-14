pub fn normalize_slug(input: &str) -> Option<String> {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for ch in input.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        None
    } else {
        Some(slug)
    }
}

#[cfg(any(feature = "server", test))]
pub fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 120
        && slug
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_slug_should_collapse_separators() {
        let slug = normalize_slug("  Wiki Page: Home!  ");

        assert_eq!(slug.as_deref(), Some("wiki-page-home"));
    }

    #[test]
    fn is_valid_slug_should_reject_path_segments() {
        assert!(!is_valid_slug("../home"));
    }
}
