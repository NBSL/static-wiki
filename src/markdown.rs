use markdown_it::MarkdownIt;
use std::sync::OnceLock;

use crate::slug::normalize_slug;

pub fn render_markdown(markdown: &str) -> String {
    parser().parse(markdown).render()
}

pub fn render_markdown_with_component_manifests(markdown: &str, manifests: &[String]) -> String {
    if manifests.is_empty() {
        return render_markdown(markdown);
    }

    let mut parser = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut parser);
    crate::markdown_components::add_rules_with_manifests(&mut parser, manifests);
    parser.parse(markdown).render()
}

fn parser() -> &'static MarkdownIt {
    static PARSER: OnceLock<MarkdownIt> = OnceLock::new();
    PARSER.get_or_init(|| {
        let mut parser = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut parser);
        crate::markdown_components::add_rules(&mut parser);
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

    let Some((front_matter, content)) = split_leading_front_matter_block(body) else {
        return compose_page_body(title, body);
    };

    let front_matter = front_matter.trim_end();
    let content = compose_page_body(title, content.trim());
    if content.is_empty() {
        format!("{front_matter}\n")
    } else {
        format!("{front_matter}\n\n{}\n", content.trim_end())
    }
}

fn compose_page_body(title: &str, body: &str) -> String {
    if title.is_empty() || body.starts_with("# ") {
        body.to_owned()
    } else if body.is_empty() {
        format!("# {title}\n")
    } else {
        format!("# {title}\n\n{body}\n")
    }
}

pub fn compose_page_markdown_with_metadata(
    title: &str,
    categories: &str,
    promoted: bool,
    body: &str,
) -> String {
    let body = body_with_metadata(categories, promoted, body);
    compose_page_markdown(title, &body)
}

pub fn editable_body_from_page_markdown(markdown: &str) -> String {
    let Some((front_matter, body)) = split_leading_front_matter(markdown) else {
        return markdown.to_owned();
    };

    front_matter_and_body_to_markdown(&front_matter_without_editor_metadata(front_matter), body)
}

#[cfg(any(feature = "server", test))]
pub fn page_body_from_markdown(markdown: &str) -> &str {
    split_leading_front_matter(markdown)
        .map(|(_front_matter, body)| body.trim_start())
        .unwrap_or(markdown)
}

pub fn categories_text_from_page_markdown(markdown: &str) -> String {
    category_slugs_from_markdown(markdown).join(", ")
}

pub fn category_slugs_from_markdown(markdown: &str) -> Vec<String> {
    let Some((front_matter, _body)) = split_leading_front_matter(markdown) else {
        return Vec::new();
    };

    category_slugs_from_front_matter(front_matter)
}

pub fn promoted_from_page_markdown(markdown: &str) -> bool {
    let Some((front_matter, _body)) = split_leading_front_matter(markdown) else {
        return true;
    };

    promoted_from_front_matter(front_matter)
}

fn body_with_metadata(categories: &str, promoted: bool, body: &str) -> String {
    let category_slugs = category_slugs_from_text(categories);
    let Some((front_matter, body)) = split_leading_front_matter(body) else {
        if category_slugs.is_empty() && promoted {
            return body.to_owned();
        }

        return front_matter_and_body_to_markdown(
            &metadata_front_matter_lines(&category_slugs, promoted),
            body,
        );
    };

    let mut front_matter_lines = metadata_front_matter_lines(&category_slugs, promoted);
    front_matter_lines.extend(front_matter_without_editor_metadata(front_matter));

    front_matter_and_body_to_markdown(&front_matter_lines, body)
}

fn metadata_front_matter_lines(category_slugs: &[String], promoted: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if !category_slugs.is_empty() {
        lines.push(category_front_matter_line(category_slugs));
    }
    if !promoted {
        lines.push("promoted: false".to_owned());
    }
    lines
}

fn category_front_matter_line(category_slugs: &[String]) -> String {
    format!("categories: [{}]", category_slugs.join(", "))
}

fn front_matter_and_body_to_markdown(front_matter_lines: &[String], body: &str) -> String {
    let front_matter_lines = trimmed_front_matter_lines(front_matter_lines);
    let body = body.trim_start();

    if front_matter_lines.is_empty() {
        return body.to_owned();
    }

    let front_matter = front_matter_lines.join("\n");
    if body.is_empty() {
        format!("---\n{front_matter}\n---\n")
    } else {
        format!("---\n{front_matter}\n---\n\n{body}")
    }
}

fn trimmed_front_matter_lines(lines: &[String]) -> Vec<String> {
    let mut start = 0;
    let mut end = lines.len();

    while start < end && lines[start].trim().is_empty() {
        start += 1;
    }
    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }

    lines[start..end].to_vec()
}

fn front_matter_without_editor_metadata(front_matter: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut reading_category_list = false;

    for line in front_matter.lines() {
        let trimmed = line.trim();
        if reading_category_list {
            if trimmed.is_empty() || trimmed.starts_with('-') {
                continue;
            }
            reading_category_list = false;
        }

        let Some((key, value)) = trimmed.split_once(':') else {
            lines.push(line.to_owned());
            continue;
        };

        if matches!(key.trim(), "category" | "categories") {
            reading_category_list = value.trim().is_empty();
            continue;
        }
        if key.trim() == "promoted" {
            continue;
        }

        lines.push(line.to_owned());
    }

    lines
}

fn category_slugs_from_front_matter(front_matter: &str) -> Vec<String> {
    let mut categories = Vec::new();
    let mut reading_category_list = false;

    for line in front_matter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if reading_category_list {
            if let Some(value) = trimmed.strip_prefix('-') {
                push_category_values(value, &mut categories);
                continue;
            }
            reading_category_list = false;
        }

        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        if !matches!(key.trim(), "category" | "categories") {
            continue;
        }

        let value = value.trim();
        if value.is_empty() {
            reading_category_list = true;
        } else {
            push_category_values(value, &mut categories);
        }
    }

    categories
}

fn promoted_from_front_matter(front_matter: &str) -> bool {
    front_matter
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }

            let (key, value) = trimmed.split_once(':')?;
            (key.trim() == "promoted").then(|| bool_from_front_matter_value(value).unwrap_or(true))
        })
        .unwrap_or(true)
}

fn bool_from_front_matter_value(value: &str) -> Option<bool> {
    let value = value.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
    match value.as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn category_slugs_from_text(value: &str) -> Vec<String> {
    let mut categories = Vec::new();
    for line in value.lines() {
        push_category_values(line, &mut categories);
    }
    categories
}

fn push_category_values(value: &str, categories: &mut Vec<String>) {
    let value = value.trim().trim_matches(['[', ']']);
    for category in value.split(',') {
        let category = category.trim().trim_matches(['"', '\'']);
        if let Some(category) = normalize_slug(category) {
            if !categories.contains(&category) {
                categories.push(category);
            }
        }
    }
}

fn split_leading_front_matter(markdown: &str) -> Option<(&str, &str)> {
    let mut offset = 0;
    let mut lines = markdown.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end_matches(['\r', '\n']).trim() != "---" {
        return None;
    }
    offset += first.len();
    let front_matter_start = offset;

    for line in lines {
        let line_start = offset;
        offset += line.len();
        if line.trim_end_matches(['\r', '\n']).trim() == "---" {
            return Some((
                &markdown[front_matter_start..line_start],
                &markdown[offset..],
            ));
        }
    }

    None
}

fn split_leading_front_matter_block(markdown: &str) -> Option<(&str, &str)> {
    let (_front_matter, body) = split_leading_front_matter(markdown)?;
    let body_start = markdown.len() - body.len();
    Some((&markdown[..body_start], body))
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

    #[test]
    fn compose_page_markdown_should_keep_front_matter_before_title() {
        let markdown =
            compose_page_markdown("Categorized", "---\ncategories: test-page\n---\n\nBody");

        assert_eq!(
            markdown,
            "---\ncategories: test-page\n---\n\n# Categorized\n\nBody\n"
        );
    }

    #[test]
    fn compose_page_markdown_with_metadata_should_write_categories_front_matter() {
        let markdown = compose_page_markdown_with_metadata("Page", "Test Page, npc", true, "Body");

        assert_eq!(
            markdown,
            "---\ncategories: [test-page, npc]\n---\n\n# Page\n\nBody\n"
        );
    }

    #[test]
    fn compose_page_markdown_with_metadata_should_replace_existing_categories() {
        let markdown = compose_page_markdown_with_metadata(
            "Page",
            "items",
            true,
            "---\ncategories:\n  - Old Page\nstatus: draft\n---\n\n# Page\n\nBody",
        );

        assert_eq!(
            markdown,
            "---\ncategories: [items]\nstatus: draft\n---\n\n# Page\n\nBody\n"
        );
    }

    #[test]
    fn compose_page_markdown_with_metadata_should_write_hidden_sidebar_state() {
        let markdown = compose_page_markdown_with_metadata("Page", "Test Page", false, "Body");

        assert_eq!(
            markdown,
            "---\ncategories: [test-page]\npromoted: false\n---\n\n# Page\n\nBody\n"
        );
    }

    #[test]
    fn compose_page_markdown_with_metadata_should_replace_existing_promoted_value() {
        let markdown = compose_page_markdown_with_metadata(
            "Page",
            "",
            false,
            "---\npromoted: true\nstatus: draft\n---\n\n# Page\n\nBody",
        );

        assert_eq!(
            markdown,
            "---\npromoted: false\nstatus: draft\n---\n\n# Page\n\nBody\n"
        );
    }

    #[test]
    fn editable_body_from_page_markdown_should_remove_category_only_front_matter() {
        let body = editable_body_from_page_markdown("---\ncategories: test-page\n---\n\n# Page");

        assert_eq!(body, "# Page");
    }

    #[test]
    fn editable_body_from_page_markdown_should_preserve_other_front_matter() {
        let body = editable_body_from_page_markdown(
            "---\ncategories: test-page\nstatus: draft\n---\nBody",
        );

        assert_eq!(body, "---\nstatus: draft\n---\n\nBody");
    }

    #[test]
    fn editable_body_from_page_markdown_should_remove_promoted_front_matter() {
        let body =
            editable_body_from_page_markdown("---\npromoted: false\nstatus: draft\n---\nBody");

        assert_eq!(body, "---\nstatus: draft\n---\n\nBody");
    }

    #[test]
    fn categories_text_from_page_markdown_should_parse_multiline_categories() {
        let categories =
            categories_text_from_page_markdown("---\ncategories:\n  - Test Page\n  - npc\n---\n");

        assert_eq!(categories, "test-page, npc");
    }

    #[test]
    fn promoted_from_page_markdown_should_default_to_true_when_missing() {
        assert!(promoted_from_page_markdown("# Page"));
    }

    #[test]
    fn promoted_from_page_markdown_should_parse_false_front_matter() {
        assert!(!promoted_from_page_markdown(
            "---\npromoted: false\n---\n\n# Page"
        ));
    }
}
