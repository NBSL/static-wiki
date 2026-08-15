use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditorMode {
    Builder,
    Markdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuilderBlockKind {
    Section,
    Paragraph,
    Image,
    TableOfContents,
    Callout,
    Infobox,
    ItemCard,
    NpcCard,
}

impl BuilderBlockKind {
    const ALL: [Self; 8] = [
        Self::Section,
        Self::Paragraph,
        Self::Image,
        Self::TableOfContents,
        Self::Callout,
        Self::Infobox,
        Self::ItemCard,
        Self::NpcCard,
    ];

    fn value(self) -> &'static str {
        match self {
            Self::Section => "section",
            Self::Paragraph => "paragraph",
            Self::Image => "image",
            Self::TableOfContents => "toc",
            Self::Callout => "callout",
            Self::Infobox => "infobox",
            Self::ItemCard => "item-card",
            Self::NpcCard => "npc-card",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Section => "Section",
            Self::Paragraph => "Paragraph",
            Self::Image => "Image",
            Self::TableOfContents => "Table of contents",
            Self::Callout => "Callout",
            Self::Infobox => "Infobox",
            Self::ItemCard => "Item card",
            Self::NpcCard => "NPC card",
        }
    }

    fn from_value(value: &str) -> Self {
        match value {
            "paragraph" => Self::Paragraph,
            "image" => Self::Image,
            "toc" => Self::TableOfContents,
            "callout" => Self::Callout,
            "infobox" => Self::Infobox,
            "item-card" => Self::ItemCard,
            "npc-card" => Self::NpcCard,
            _ => Self::Section,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BuilderDraft {
    kind: BuilderBlockKind,
    title: String,
    body: String,
    image: String,
    alt: String,
    caption: String,
    rows: String,
    list_items: String,
    tags: String,
    icon: String,
}

fn markdown_block_from_builder_draft(draft: &BuilderDraft) -> String {
    match draft.kind {
        BuilderBlockKind::Section => section_markdown(draft),
        BuilderBlockKind::Paragraph => {
            defaulted(&draft.body, "Write page content here.").to_owned()
        }
        BuilderBlockKind::Image => image_markdown(draft),
        BuilderBlockKind::TableOfContents => table_of_contents_markdown(draft),
        BuilderBlockKind::Callout => callout_markdown(draft),
        BuilderBlockKind::Infobox => infobox_markdown(draft),
        BuilderBlockKind::ItemCard => item_card_markdown(draft),
        BuilderBlockKind::NpcCard => npc_card_markdown(draft),
    }
}

fn section_markdown(draft: &BuilderDraft) -> String {
    let title = defaulted(&draft.title, "New section");
    let body = draft.body.trim();
    if body.is_empty() {
        format!("## {title}")
    } else {
        format!("## {title}\n\n{body}")
    }
}

fn image_markdown(draft: &BuilderDraft) -> String {
    let image = defaulted(&draft.image, "image.png");
    let alt = defaulted(&draft.alt, "Image");
    let caption = draft.caption.trim();
    let mut markdown = format!("![{alt}]({image})");
    if !caption.is_empty() {
        markdown.push_str("\n\n*");
        markdown.push_str(caption);
        markdown.push('*');
    }
    markdown
}

fn table_of_contents_markdown(draft: &BuilderDraft) -> String {
    let title = defaulted(&draft.title, "Contents");
    format!("```toc\ntitle: {title}\nmin-depth: 2\nmax-depth: 4\nordered: false\n```")
}

fn callout_markdown(draft: &BuilderDraft) -> String {
    let mut lines = Vec::new();
    push_field(&mut lines, "title", defaulted(&draft.title, "Note"));
    push_optional_field(&mut lines, "image", &draft.image);
    push_optional_field(&mut lines, "alt", &draft.alt);
    push_repeated_text_fields(&mut lines, "body", &draft.body, Some("Callout text."));
    if let Some(items) = pipe_list_value(&draft.list_items) {
        push_field(&mut lines, "items", &items);
    }
    push_raw_component_rows(&mut lines, &draft.rows);
    component_fence("callout", &lines)
}

fn infobox_markdown(draft: &BuilderDraft) -> String {
    let mut lines = Vec::new();
    push_field(&mut lines, "title", defaulted(&draft.title, "Infobox"));
    push_optional_field(&mut lines, "image", &draft.image);
    push_optional_field(&mut lines, "alt", &draft.alt);
    push_optional_field(&mut lines, "caption", &draft.caption);
    push_raw_component_rows(&mut lines, &draft.rows);
    component_fence("infobox", &lines)
}

fn item_card_markdown(draft: &BuilderDraft) -> String {
    let mut lines = Vec::new();
    push_field(&mut lines, "title", defaulted(&draft.title, "New item"));
    push_field(&mut lines, "icon", defaulted(&draft.icon, "necklace"));
    if let Some(tags) = comma_list_value(&draft.tags) {
        push_field(&mut lines, "tags", &tags);
    }
    push_repeated_text_fields(
        &mut lines,
        "description",
        &draft.body,
        Some("Item description."),
    );
    push_repeated_line_fields(&mut lines, "line", &draft.rows, Some("Slot: NECK"));
    component_fence("item-card", &lines)
}

fn npc_card_markdown(draft: &BuilderDraft) -> String {
    let mut lines = Vec::new();
    push_field(&mut lines, "name", defaulted(&draft.title, "New NPC"));
    push_optional_field(&mut lines, "portrait", &draft.image);
    push_optional_field(&mut lines, "role", &draft.caption);
    if let Some(traits) = pipe_list_value(&draft.list_items) {
        push_field(&mut lines, "traits", &traits);
    }
    push_raw_component_rows(&mut lines, &draft.rows);
    component_fence("npc-card", &lines)
}

fn append_markdown_block(markdown: &str, block: &str) -> String {
    let block = block.trim();
    if block.is_empty() {
        return markdown.to_owned();
    }

    let mut next = markdown.trim_end().to_owned();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(block);
    next.push('\n');
    next
}

fn defaulted<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    let value = value.trim();
    if value.is_empty() {
        fallback
    } else {
        value
    }
}

fn trimmed_lines(value: &str) -> impl Iterator<Item = &str> {
    value.lines().map(str::trim).filter(|line| !line.is_empty())
}

fn push_field(lines: &mut Vec<String>, key: &str, value: &str) {
    lines.push(format!("{key}: {}", value.trim()));
}

fn push_optional_field(lines: &mut Vec<String>, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        push_field(lines, key, value);
    }
}

fn push_repeated_text_fields(
    lines: &mut Vec<String>,
    key: &str,
    value: &str,
    fallback: Option<&str>,
) {
    let mut added = false;
    for line in trimmed_lines(value) {
        push_field(lines, key, line);
        added = true;
    }

    if !added {
        if let Some(fallback) = fallback {
            push_field(lines, key, fallback);
        }
    }
}

fn push_repeated_line_fields(
    lines: &mut Vec<String>,
    key: &str,
    value: &str,
    fallback: Option<&str>,
) {
    let mut added = false;
    for line in trimmed_lines(value) {
        push_field(lines, key, line);
        added = true;
    }

    if !added {
        if let Some(fallback) = fallback {
            push_field(lines, key, fallback);
        }
    }
}

fn push_raw_component_rows(lines: &mut Vec<String>, value: &str) {
    lines.extend(trimmed_lines(value).map(ToOwned::to_owned));
}

fn component_fence(fence: &str, lines: &[String]) -> String {
    format!("```{fence}\n{}\n```", lines.join("\n"))
}

fn pipe_list_value(value: &str) -> Option<String> {
    delimited_list_value(value, '|', " | ")
}

fn comma_list_value(value: &str) -> Option<String> {
    delimited_list_value(value, ',', ", ")
}

fn delimited_list_value(value: &str, delimiter: char, joiner: &str) -> Option<String> {
    let entries = value
        .lines()
        .flat_map(|line| line.split(delimiter))
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();
    (!entries.is_empty()).then(|| entries.join(joiner))
}

#[component]
pub(crate) fn ModeButton(
    label: &'static str,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if active {
        "inline-flex h-9 items-center rounded-md border border-emerald-700 bg-emerald-700 px-3 text-sm font-semibold text-white"
    } else {
        "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400"
    };

    rsx! {
        button { class, onclick: move |event| onclick.call(event), "{label}" }
    }
}

#[component]
pub(crate) fn PageBuilder(editor_markdown: Signal<String>) -> Element {
    let mut block_kind = use_signal(|| BuilderBlockKind::Section);
    let mut title = use_signal(String::new);
    let mut body = use_signal(String::new);
    let mut image = use_signal(String::new);
    let mut alt = use_signal(String::new);
    let mut caption = use_signal(String::new);
    let mut rows = use_signal(String::new);
    let mut list_items = use_signal(String::new);
    let mut tags = use_signal(String::new);
    let mut icon = use_signal(String::new);
    let mut message = use_signal(String::new);
    let selected_kind = block_kind();
    let message_text = message();
    let title_label = if selected_kind == BuilderBlockKind::NpcCard {
        "Name"
    } else {
        "Title"
    };
    let title_placeholder = if selected_kind == BuilderBlockKind::NpcCard {
        "NPC name"
    } else {
        "Block title"
    };

    let add_block = move |_| {
        let draft = BuilderDraft {
            kind: block_kind(),
            title: title(),
            body: body(),
            image: image(),
            alt: alt(),
            caption: caption(),
            rows: rows(),
            list_items: list_items(),
            tags: tags(),
            icon: icon(),
        };
        let block = markdown_block_from_builder_draft(&draft);
        editor_markdown.set(append_markdown_block(&editor_markdown(), &block));
        message.set("Block added.".to_owned());
    };

    let clear_form = move |_| {
        title.set(String::new());
        body.set(String::new());
        image.set(String::new());
        alt.set(String::new());
        caption.set(String::new());
        rows.set(String::new());
        list_items.set(String::new());
        tags.set(String::new());
        icon.set(String::new());
        message.set(String::new());
    };

    rsx! {
        div { class: "grid gap-4",
            div { class: "grid gap-3 md:grid-cols-2",
                label { class: "grid gap-1 text-sm font-semibold text-slate-700",
                    "Block"
                    select {
                        value: "{selected_kind.value()}",
                        oninput: move |event| {
                            block_kind.set(BuilderBlockKind::from_value(&event.value()));
                            message.set(String::new());
                        },
                        for kind in BuilderBlockKind::ALL {
                            option {
                                value: "{kind.value()}",
                                "{kind.label()}"
                            }
                        }
                    }
                }
                if matches!(selected_kind, BuilderBlockKind::Section | BuilderBlockKind::TableOfContents | BuilderBlockKind::Callout | BuilderBlockKind::Infobox | BuilderBlockKind::ItemCard | BuilderBlockKind::NpcCard) {
                    BuilderTextInput {
                        label: title_label,
                        value: title,
                        placeholder: title_placeholder
                    }
                }
            }

            match selected_kind {
                BuilderBlockKind::Section => rsx! {
                    BuilderTextarea {
                        label: "Body",
                        value: body,
                        placeholder: "Section content"
                    }
                },
                BuilderBlockKind::Paragraph => rsx! {
                    BuilderTextarea {
                        label: "Paragraph",
                        value: body,
                        placeholder: "Paragraph text"
                    }
                },
                BuilderBlockKind::Image => rsx! {
                    div { class: "grid gap-3 md:grid-cols-2",
                        BuilderTextInput {
                            label: "Image",
                            value: image,
                            placeholder: "filename.jpg or https://..."
                        }
                        BuilderTextInput {
                            label: "Alt",
                            value: alt,
                            placeholder: "Image description"
                        }
                    }
                    BuilderTextInput {
                        label: "Caption",
                        value: caption,
                        placeholder: "Optional caption"
                    }
                },
                BuilderBlockKind::TableOfContents => rsx! {},
                BuilderBlockKind::Callout => rsx! {
                    BuilderTextarea {
                        label: "Body",
                        value: body,
                        placeholder: "Callout text"
                    }
                    div { class: "grid gap-3 md:grid-cols-2",
                        BuilderTextInput {
                            label: "Image",
                            value: image,
                            placeholder: "Optional image"
                        }
                        BuilderTextInput {
                            label: "Alt",
                            value: alt,
                            placeholder: "Image description"
                        }
                    }
                    BuilderTextarea {
                        label: "Items",
                        value: list_items,
                        placeholder: "One item per line"
                    }
                    BuilderTextarea {
                        label: "Fields",
                        value: rows,
                        placeholder: "Status: Draft"
                    }
                },
                BuilderBlockKind::Infobox => rsx! {
                    div { class: "grid gap-3 md:grid-cols-2",
                        BuilderTextInput {
                            label: "Image",
                            value: image,
                            placeholder: "Optional image"
                        }
                        BuilderTextInput {
                            label: "Alt",
                            value: alt,
                            placeholder: "Image description"
                        }
                    }
                    BuilderTextInput {
                        label: "Caption",
                        value: caption,
                        placeholder: "Optional caption"
                    }
                    BuilderTextarea {
                        label: "Fields",
                        value: rows,
                        placeholder: "Born: 10 July 1856"
                    }
                },
                BuilderBlockKind::ItemCard => rsx! {
                    div { class: "grid gap-3 md:grid-cols-2",
                        BuilderTextInput {
                            label: "Icon",
                            value: icon,
                            placeholder: "necklace"
                        }
                        BuilderTextInput {
                            label: "Tags",
                            value: tags,
                            placeholder: "MAGIC, UNIQUE"
                        }
                    }
                    BuilderTextarea {
                        label: "Description",
                        value: body,
                        placeholder: "Item description"
                    }
                    BuilderTextarea {
                        label: "Lines",
                        value: rows,
                        placeholder: "Slot: NECK"
                    }
                },
                BuilderBlockKind::NpcCard => rsx! {
                    div { class: "grid gap-3 md:grid-cols-2",
                        BuilderTextInput {
                            label: "Portrait",
                            value: image,
                            placeholder: "filename.jpg or https://..."
                        }
                        BuilderTextInput {
                            label: "Role",
                            value: caption,
                            placeholder: "Harbor Guard"
                        }
                    }
                    BuilderTextarea {
                        label: "Traits",
                        value: list_items,
                        placeholder: "One trait per line"
                    }
                    BuilderTextarea {
                        label: "Fields",
                        value: rows,
                        placeholder: "Faction: Port Authority"
                    }
                },
            }

            div { class: "flex flex-wrap items-center gap-2",
                button {
                    class: "inline-flex h-9 items-center rounded-md border border-emerald-700 bg-emerald-700 px-3 text-sm font-semibold text-white hover:bg-emerald-800",
                    onclick: add_block,
                    "Add block"
                }
                button {
                    class: "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400",
                    onclick: clear_form,
                    "Clear"
                }
                if !message_text.is_empty() {
                    span { class: "text-sm text-slate-500", "{message_text}" }
                }
            }
        }
    }
}

#[component]
fn BuilderTextInput(
    label: &'static str,
    value: Signal<String>,
    placeholder: &'static str,
) -> Element {
    rsx! {
        label { class: "grid gap-1 text-sm font-semibold text-slate-700",
            "{label}"
            input {
                value: "{value}",
                oninput: move |event| value.set(event.value()),
                placeholder
            }
        }
    }
}

#[component]
fn BuilderTextarea(
    label: &'static str,
    value: Signal<String>,
    placeholder: &'static str,
) -> Element {
    rsx! {
        label { class: "grid gap-1 text-sm font-semibold text-slate-700",
            "{label}"
            textarea {
                class: "min-h-28 font-sans leading-5",
                value: "{value}",
                oninput: move |event| value.set(event.value()),
                placeholder
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(kind: BuilderBlockKind) -> BuilderDraft {
        BuilderDraft {
            kind,
            title: String::new(),
            body: String::new(),
            image: String::new(),
            alt: String::new(),
            caption: String::new(),
            rows: String::new(),
            list_items: String::new(),
            tags: String::new(),
            icon: String::new(),
        }
    }

    #[test]
    fn append_markdown_block_should_separate_existing_content() {
        let markdown = append_markdown_block("# Page\n", "## Section");

        assert_eq!(markdown, "# Page\n\n## Section\n");
    }

    #[test]
    fn markdown_block_from_builder_draft_should_create_callout_fence() {
        let mut draft = draft(BuilderBlockKind::Callout);
        draft.title = "Note".to_owned();
        draft.body = "First line\nSecond line".to_owned();
        draft.list_items = "Static\nSafe".to_owned();
        draft.rows = "Status: Draft".to_owned();

        let markdown = markdown_block_from_builder_draft(&draft);

        assert_eq!(
            markdown,
            "```callout\ntitle: Note\nbody: First line\nbody: Second line\nitems: Static | Safe\nStatus: Draft\n```"
        );
    }

    #[test]
    fn markdown_block_from_builder_draft_should_create_item_card_fence() {
        let mut draft = draft(BuilderBlockKind::ItemCard);
        draft.title = "Amulet".to_owned();
        draft.icon = "necklace".to_owned();
        draft.tags = "MAGIC\nUNIQUE".to_owned();
        draft.body = "A simple pendant.".to_owned();
        draft.rows = "Slot: NECK\nMana: +30".to_owned();

        let markdown = markdown_block_from_builder_draft(&draft);

        assert_eq!(
            markdown,
            "```item-card\ntitle: Amulet\nicon: necklace\ntags: MAGIC, UNIQUE\ndescription: A simple pendant.\nline: Slot: NECK\nline: Mana: +30\n```"
        );
    }

    #[test]
    fn markdown_block_from_builder_draft_should_create_npc_card_fence() {
        let mut draft = draft(BuilderBlockKind::NpcCard);
        draft.title = "Captain Veyra".to_owned();
        draft.image = "veyra.png".to_owned();
        draft.caption = "Harbor Guard".to_owned();
        draft.list_items = "Stern\nLoyal".to_owned();
        draft.rows = "Faction: Port Authority".to_owned();

        let markdown = markdown_block_from_builder_draft(&draft);

        assert_eq!(
            markdown,
            "```npc-card\nname: Captain Veyra\nportrait: veyra.png\nrole: Harbor Guard\ntraits: Stern | Loyal\nFaction: Port Authority\n```"
        );
    }
}
