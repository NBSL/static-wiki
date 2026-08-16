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

    if let Some((front_matter, content)) = split_leading_front_matter_block(body) {
        let content = content.trim();
        let front_matter = front_matter.trim_end();
        if title.is_empty() || content.starts_with("# ") {
            if content.is_empty() {
                format!("{front_matter}\n")
            } else {
                format!("{front_matter}\n\n{content}\n")
            }
        } else if content.is_empty() {
            format!("{front_matter}\n\n# {title}\n")
        } else {
            format!("{front_matter}\n\n# {title}\n\n{content}\n")
        }
    } else if title.is_empty() || body.starts_with("# ") {
        body.to_owned()
    } else if body.is_empty() {
        format!("# {title}\n")
    } else {
        format!("# {title}\n\n{body}\n")
    }
}

pub fn compose_page_markdown_with_categories(title: &str, categories: &str, body: &str) -> String {
    let body = body_with_categories(categories, body);
    compose_page_markdown(title, &body)
}

pub fn editable_body_from_page_markdown(markdown: &str) -> String {
    let Some((front_matter, body)) = split_leading_front_matter(markdown) else {
        return markdown.to_owned();
    };

    front_matter_and_body_to_markdown(&front_matter_without_categories(front_matter), body)
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

fn body_with_categories(categories: &str, body: &str) -> String {
    let category_slugs = category_slugs_from_text(categories);
    let Some((front_matter, body)) = split_leading_front_matter(body) else {
        if category_slugs.is_empty() {
            return body.to_owned();
        }

        return front_matter_and_body_to_markdown(
            &[category_front_matter_line(&category_slugs)],
            body,
        );
    };

    let mut front_matter_lines = Vec::new();
    if !category_slugs.is_empty() {
        front_matter_lines.push(category_front_matter_line(&category_slugs));
    }
    front_matter_lines.extend(front_matter_without_categories(front_matter));

    front_matter_and_body_to_markdown(&front_matter_lines, body)
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

fn front_matter_without_categories(front_matter: &str) -> Vec<String> {
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
    let mut offset = 0;
    let mut lines = markdown.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end_matches(['\r', '\n']).trim() != "---" {
        return None;
    }
    offset += first.len();

    for line in lines {
        offset += line.len();
        if line.trim_end_matches(['\r', '\n']).trim() == "---" {
            return Some((&markdown[..offset], &markdown[offset..]));
        }
    }

    None
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
    fn compose_page_markdown_with_categories_should_write_front_matter() {
        let markdown = compose_page_markdown_with_categories("Page", "Test Page, npc", "Body");

        assert_eq!(
            markdown,
            "---\ncategories: [test-page, npc]\n---\n\n# Page\n\nBody\n"
        );
    }

    #[test]
    fn compose_page_markdown_with_categories_should_replace_existing_categories() {
        let markdown = compose_page_markdown_with_categories(
            "Page",
            "items",
            "---\ncategories:\n  - Old Page\nstatus: draft\n---\n\n# Page\n\nBody",
        );

        assert_eq!(
            markdown,
            "---\ncategories: [items]\nstatus: draft\n---\n\n# Page\n\nBody\n"
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
    fn categories_text_from_page_markdown_should_parse_multiline_categories() {
        let categories =
            categories_text_from_page_markdown("---\ncategories:\n  - Test Page\n  - npc\n---\n");

        assert_eq!(categories, "test-page, npc");
    }
}
