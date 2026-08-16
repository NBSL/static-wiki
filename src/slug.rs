use std::fmt;

pub const MAX_PAGE_TITLE_CHARS: usize = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageValidationError {
    EmptyTitle,
    TitleTooLong,
    EmptySlug,
    InvalidSlug,
}

impl fmt::Display for PageValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyTitle => "page title cannot be empty",
            Self::TitleTooLong => "page title cannot exceed 200 characters",
            Self::EmptySlug => "page slug cannot be empty",
            Self::InvalidSlug => "page slug is invalid or exceeds 120 characters",
        };
        formatter.write_str(message)
    }
}

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

pub fn validate_page_title(title: &str) -> Result<(), PageValidationError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(PageValidationError::EmptyTitle);
    }
    if title.chars().count() > MAX_PAGE_TITLE_CHARS {
        return Err(PageValidationError::TitleTooLong);
    }
    Ok(())
}

pub fn normalize_page_slug(input: &str) -> Result<String, PageValidationError> {
    let slug = normalize_slug(input).ok_or(PageValidationError::EmptySlug)?;
    if !is_valid_slug(&slug) {
        return Err(PageValidationError::InvalidSlug);
    }
    Ok(slug)
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
    fn validate_page_title_should_reject_empty_titles() {
        assert_eq!(
            validate_page_title("  "),
            Err(PageValidationError::EmptyTitle)
        );
    }

    #[test]
    fn validate_page_title_should_reject_titles_over_the_limit() {
        let title = "a".repeat(MAX_PAGE_TITLE_CHARS + 1);

        assert_eq!(
            validate_page_title(&title),
            Err(PageValidationError::TitleTooLong)
        );
    }

    #[test]
    fn normalize_page_slug_should_accept_normalizable_input() {
        let slug = normalize_page_slug("Wiki Page");

        assert_eq!(slug.as_deref(), Ok("wiki-page"));
    }

    #[test]
    fn normalize_page_slug_should_reject_slugs_over_the_limit() {
        let slug = "a".repeat(121);

        assert_eq!(
            normalize_page_slug(&slug),
            Err(PageValidationError::InvalidSlug)
        );
    }

    #[test]
    fn is_valid_slug_should_reject_path_segments() {
        assert!(!is_valid_slug("../home"));
    }
}
