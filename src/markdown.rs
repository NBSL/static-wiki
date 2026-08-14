use std::sync::OnceLock;

pub fn render_markdown(markdown: &str) -> String {
    parser().parse(markdown).render()
}

fn parser() -> &'static markdown_it::MarkdownIt {
    static PARSER: OnceLock<markdown_it::MarkdownIt> = OnceLock::new();
    PARSER.get_or_init(|| {
        let mut parser = markdown_it::MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut parser);
        parser
    })
}

#[cfg(any(feature = "server", test))]
pub fn title_from_markdown(markdown: &str, fallback: &str) -> String {
    markdown
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| humanize_slug(fallback))
}

pub fn compose_page_markdown(title: &str, body: &str) -> String {
    let title = title.trim();
    let body = body.trim();
    if title.is_empty() || body.starts_with("# ") {
        body.to_owned()
    } else if body.is_empty() {
        format!("# {title}\n")
    } else {
        format!("# {title}\n\n{body}\n")
    }
}

#[cfg(any(feature = "server", test))]
pub fn humanize_slug(slug: &str) -> String {
    let mut title = String::new();
    for word in slug.split(['-', '_']).filter(|word| !word.is_empty()) {
        if !title.is_empty() {
            title.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            title.extend(first.to_uppercase());
            title.push_str(chars.as_str());
        }
    }
    if title.is_empty() {
        "Untitled".to_owned()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_markdown_should_strip_raw_html() {
        let html = render_markdown("# Title\n\n<script>alert('x')</script>\n\n**safe**");

        assert!(!html.contains("<script>"));
    }

    #[test]
    fn title_from_markdown_should_use_first_h1() {
        let title = title_from_markdown("intro\n# Real Title\nbody", "fallback");

        assert_eq!(title, "Real Title");
    }

    #[test]
    fn compose_page_markdown_should_preserve_existing_heading() {
        let markdown = compose_page_markdown("Ignored", "# Existing\n\nBody");

        assert_eq!(markdown, "# Existing\n\nBody");
    }
}
