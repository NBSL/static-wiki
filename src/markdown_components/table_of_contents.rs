use std::collections::HashMap;

use markdown_it::{
    parser::core::CoreRule,
    plugins::cmark::block::{fence::CodeFence, heading::ATXHeading, lheading::SetextHeader},
    MarkdownIt, Node, NodeValue, Renderer,
};

#[derive(Debug)]
struct TableOfContents {
    title: Option<String>,
    ordered: bool,
    min_level: u8,
    items: Vec<TocItem>,
}

#[derive(Debug, Clone)]
struct TocItem {
    level: u8,
    title: String,
    id: String,
}

#[derive(Debug)]
struct TocOptions {
    title: Option<String>,
    min_level: u8,
    max_level: u8,
    ordered: bool,
}

impl Default for TocOptions {
    fn default() -> Self {
        Self {
            title: Some("Contents".to_owned()),
            min_level: 2,
            max_level: 6,
            ordered: false,
        }
    }
}

impl NodeValue for TableOfContents {
    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        if self.items.is_empty() {
            return;
        }

        fmt.cr();
        fmt.open(
            "nav",
            &[
                ("class", "toc".to_owned()),
                ("aria-label", "Table of contents".to_owned()),
            ],
        );

        if let Some(title) = &self.title {
            fmt.open("div", &[("class", "toc-title".to_owned())]);
            fmt.text(title);
            fmt.close("div");
        }

        let list_tag = if self.ordered { "ol" } else { "ul" };
        let list_class = if self.ordered {
            "toc-list toc-list-ordered"
        } else {
            "toc-list toc-list-unordered"
        };
        fmt.open(list_tag, &[("class", list_class.to_owned())]);
        for item in &self.items {
            let depth = item.level.saturating_sub(self.min_level);
            fmt.open(
                "li",
                &[(
                    "class",
                    format!("toc-item toc-level-{} toc-depth-{depth}", item.level),
                )],
            );
            fmt.open("a", &[("href", format!("#{}", item.id))]);
            fmt.text(&item.title);
            fmt.close("a");
            fmt.close("li");
        }
        fmt.close(list_tag);
        fmt.close("nav");
        fmt.cr();
    }
}

pub(super) struct TableOfContentsRule;

impl CoreRule for TableOfContentsRule {
    fn run(root: &mut Node, _: &MarkdownIt) {
        if !contains_toc_fence(root) {
            return;
        }

        let headings = collect_headings(root);

        root.walk_mut(|node, _| {
            let Some(fence) = node.cast::<CodeFence>() else {
                return;
            };
            if !is_toc_fence(&fence.info) {
                return;
            }

            let options = parse_toc_options(&fence.content);
            let items = headings
                .iter()
                .filter(|item| item.level >= options.min_level && item.level <= options.max_level)
                .cloned()
                .collect::<Vec<_>>();

            node.replace(TableOfContents {
                title: options.title,
                ordered: options.ordered,
                min_level: options.min_level,
                items,
            });
        });
    }
}

fn contains_toc_fence(root: &Node) -> bool {
    let mut has_toc = false;

    root.walk(|node, _| {
        if has_toc {
            return;
        }

        let Some(fence) = node.cast::<CodeFence>() else {
            return;
        };
        has_toc = is_toc_fence(&fence.info);
    });

    has_toc
}

fn collect_headings(root: &mut Node) -> Vec<TocItem> {
    let mut headings = Vec::new();
    let mut used_ids = HashMap::new();

    root.walk_mut(|node, _| {
        let Some(level) = heading_level(node) else {
            return;
        };

        let title = node.collect_text().trim().to_owned();
        if title.is_empty() {
            return;
        }

        let id = heading_id(&title, &mut used_ids);
        set_id_attr(node, &id);
        headings.push(TocItem { level, title, id });
    });

    headings
}

fn heading_level(node: &Node) -> Option<u8> {
    if let Some(heading) = node.cast::<ATXHeading>() {
        Some(heading.level)
    } else {
        node.cast::<SetextHeader>().map(|heading| heading.level)
    }
}

fn heading_id(title: &str, used_ids: &mut HashMap<String, usize>) -> String {
    let base = crate::slug::normalize_slug(title).unwrap_or_else(|| "section".to_owned());
    let count = used_ids.entry(base.clone()).or_insert(0);
    *count += 1;

    if *count == 1 {
        base
    } else {
        format!("{base}-{count}")
    }
}

fn set_id_attr(node: &mut Node, id: &str) {
    if let Some((_, value)) = node.attrs.iter_mut().find(|(name, _)| *name == "id") {
        *value = id.to_owned();
    } else {
        node.attrs.push(("id", id.to_owned()));
    }
}

fn is_toc_fence(info: &str) -> bool {
    matches!(
        info.split_whitespace().next(),
        Some("toc" | "table-of-contents" | "table_of_contents")
    )
}

fn parse_toc_options(content: &str) -> TocOptions {
    let mut options = TocOptions::default();

    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((label, value)) = line.split_once(':') else {
            continue;
        };
        let label = label.trim();
        let value = value.trim();
        if label.is_empty() || value.is_empty() {
            continue;
        }

        match normalized_toc_key(label).as_str() {
            "title" | "name" => options.title = toc_title(value),
            "min" | "min_depth" | "min_level" | "from" => {
                if let Some(level) = parse_heading_level(value) {
                    options.min_level = level;
                }
            }
            "max" | "max_depth" | "max_level" | "depth" | "to" => {
                if let Some(level) = parse_heading_level(value) {
                    options.max_level = level;
                }
            }
            "ordered" | "numbered" => options.ordered = parse_toc_bool(value),
            _ => {}
        }
    }

    if options.min_level > options.max_level {
        std::mem::swap(&mut options.min_level, &mut options.max_level);
    }

    options
}

fn normalized_toc_key(key: &str) -> String {
    key.trim().to_lowercase().replace([' ', '-'], "_")
}

fn toc_title(value: &str) -> Option<String> {
    let value = value.trim();
    if matches!(
        value.to_ascii_lowercase().as_str(),
        "false" | "none" | "off" | "no"
    ) {
        None
    } else {
        Some(value.to_owned())
    }
}

fn parse_heading_level(value: &str) -> Option<u8> {
    let level = value
        .trim()
        .trim_start_matches(['h', 'H'])
        .parse::<u8>()
        .ok()?;
    (1..=6).contains(&level).then_some(level)
}

fn parse_toc_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "yes" | "on" | "1" | "ordered" | "numbered"
    )
}

#[cfg(test)]
mod tests {
    use crate::markdown::render_markdown;

    #[test]
    fn render_markdown_should_render_toc_fence() {
        let html = render_markdown("```toc\n```\n\n# Page\n\n## Overview\n\n### Details");

        assert!(html.contains(r#"<nav class="toc" aria-label="Table of contents">"#));
        assert!(html.contains(r##"<a href="#overview">Overview</a>"##));
        assert!(html.contains(r#"<h2 id="overview">Overview</h2>"#));
    }

    #[test]
    fn render_markdown_should_make_duplicate_toc_ids_unique() {
        let html = render_markdown("```toc\n```\n\n## Setup\n\n## Setup");

        assert!(html.contains(r##"<a href="#setup">Setup</a>"##));
        assert!(html.contains(r##"<a href="#setup-2">Setup</a>"##));
        assert!(html.contains(r#"<h2 id="setup-2">Setup</h2>"#));
    }

    #[test]
    fn render_markdown_should_respect_toc_options() {
        let html = render_markdown(
            "```toc\ntitle: On This Page\nmin-depth: 3\nmax-depth: 3\nordered: true\n```\n\n## Excluded\n\n### Included",
        );

        assert!(html.contains(r#"<div class="toc-title">On This Page</div>"#));
        assert!(html.contains(r#"<ol class="toc-list toc-list-ordered">"#));
        assert!(!html.contains(r##"<a href="#excluded">Excluded</a>"##));
        assert!(html.contains(r##"<a href="#included">Included</a>"##));
    }

    #[test]
    fn render_markdown_should_escape_toc_title() {
        let html = render_markdown("```toc\ntitle: <b>Contents</b>\n```\n\n## Safe");

        assert!(!html.contains("<b>Contents</b>"));
        assert!(html.contains("&lt;b&gt;Contents&lt;/b&gt;"));
    }
}
