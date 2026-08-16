use crate::media::list_media_entries;
use crate::models::{MediaEntry, MediaEntryKind};
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct BuilderMediaCrumb {
    label: String,
    path: String,
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
    push_repeated_fields(&mut lines, "body", &draft.body, Some("Callout text."));
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
    push_repeated_fields(
        &mut lines,
        "description",
        &draft.body,
        Some("Item description."),
    );
    push_repeated_fields(&mut lines, "line", &draft.rows, Some("Slot: NECK"));
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

fn push_repeated_fields(lines: &mut Vec<String>, key: &str, value: &str, fallback: Option<&str>) {
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
                        BuilderMediaImagePicker {
                            label: "Image",
                            value: image,
                            empty_label: "No image selected",
                            dialog_title: "Choose Image",
                            preview_alt: "Selected image"
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
                        BuilderMediaImagePicker {
                            label: "Portrait",
                            value: image,
                            empty_label: "No portrait selected",
                            dialog_title: "Choose Portrait",
                            preview_alt: "Selected portrait"
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

#[component]
fn BuilderMediaImagePicker(
    label: &'static str,
    mut value: Signal<String>,
    empty_label: &'static str,
    dialog_title: &'static str,
    preview_alt: &'static str,
) -> Element {
    let mut picker_path = use_signal(String::new);
    let mut refresh_key = use_signal(|| 0_u64);
    let mut picker_open = use_signal(|| false);
    let media_resource = use_resource(move || async move {
        let _ = refresh_key();
        list_media_entries(picker_path()).await
    });
    let selected_value = value();
    let selected_is_empty = selected_value.trim().is_empty();
    let selected_label = if selected_is_empty {
        empty_label.to_owned()
    } else {
        selected_value.clone()
    };
    let preview_src = builder_media_preview_src(&selected_value);
    let current_path = picker_path();
    let breadcrumbs = builder_media_breadcrumbs(&current_path);
    let media_state = media_resource();

    rsx! {
        div { class: "grid gap-2 text-sm font-semibold text-slate-700",
            span { "{label}" }
            div { class: "flex min-h-20 items-center gap-3 rounded-md border border-stone-300 bg-white p-2",
                match preview_src {
                    Some(src) => rsx! {
                        div { class: "h-16 w-16 shrink-0 overflow-hidden rounded-md bg-stone-100",
                            img {
                                class: "h-full w-full object-cover",
                                src: "{src}",
                                alt: "{preview_alt}"
                            }
                        }
                    },
                    None => rsx! {
                        div { class: "flex h-16 w-16 shrink-0 items-center justify-center rounded-md bg-stone-100 text-xs font-semibold text-slate-500",
                            "None"
                        }
                    },
                }
                div { class: "min-w-0 flex-1",
                    span { class: "block truncate text-sm font-semibold text-slate-900", "{selected_label}" }
                }
                button {
                    class: "inline-flex h-9 shrink-0 items-center rounded-md border border-emerald-700 bg-emerald-700 px-3 text-sm font-semibold text-white hover:bg-emerald-800",
                    onclick: move |_| picker_open.set(true),
                    "Choose"
                }
                if !selected_is_empty {
                    button {
                        class: "inline-flex h-9 shrink-0 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400",
                        onclick: move |_| value.set(String::new()),
                        "Clear"
                    }
                }
            }

            if picker_open() {
                div { class: "fixed inset-0 z-50 flex items-center justify-center bg-slate-950/50 p-4",
                    div { class: "grid max-h-[90vh] w-full max-w-3xl grid-rows-[auto_auto_minmax(0,1fr)] overflow-hidden rounded-lg bg-white shadow-xl",
                        div { class: "flex items-center justify-between gap-3 border-b border-stone-200 px-4 py-3",
                            div { class: "min-w-0",
                                h3 { class: "truncate text-base font-semibold text-slate-950", "{dialog_title}" }
                                p { class: "truncate text-xs font-medium text-slate-500", "{current_path}" }
                            }
                            button {
                                class: "inline-flex h-9 shrink-0 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400",
                                onclick: move |_| picker_open.set(false),
                                "Close"
                            }
                        }

                        div { class: "flex flex-wrap items-center justify-between gap-2 border-b border-stone-200 px-4 py-3",
                            div { class: "flex flex-wrap items-center gap-1 text-xs",
                                for crumb in breadcrumbs {
                                    BuilderMediaCrumbButton {
                                        key: "{crumb.path}",
                                        crumb,
                                        current_path: current_path.clone(),
                                        on_select: move |path: String| picker_path.set(path),
                                    }
                                }
                            }
                            button {
                                class: "inline-flex h-8 items-center rounded-md border border-stone-300 bg-white px-2 text-xs font-semibold text-slate-800 hover:border-stone-400",
                                onclick: move |_| refresh_key.set(refresh_key() + 1),
                                "Refresh"
                            }
                        }

                        div { class: "overflow-y-auto bg-stone-50 p-4",
                            match media_state {
                                Some(Ok(listing)) => {
                                    let entries = listing
                                        .entries
                                        .into_iter()
                                        .filter(media_entry_is_builder_picker_option)
                                        .collect::<Vec<_>>();
                                    rsx! {
                                        if entries.is_empty() {
                                            div { class: "rounded-md border border-dashed border-stone-300 bg-white p-6 text-sm text-slate-500",
                                                "No folders or images in this folder"
                                            }
                                        } else {
                                            div { class: "grid gap-3 sm:grid-cols-2 lg:grid-cols-3",
                                                for entry in entries {
                                                    BuilderMediaPickerEntry {
                                                        key: "{entry.path}",
                                                        entry,
                                                        selected_path: selected_value.clone(),
                                                        on_open_folder: move |path: String| picker_path.set(path),
                                                        on_select_image: move |path: String| {
                                                            value.set(path);
                                                            picker_open.set(false);
                                                        },
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                Some(Err(err)) => rsx! {
                                    p { class: "rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-800", "{err}" }
                                },
                                None => rsx! {
                                    p { class: "rounded-md border border-stone-200 bg-white p-3 text-sm text-slate-500", "Loading media" }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BuilderMediaCrumbButton(
    crumb: BuilderMediaCrumb,
    current_path: String,
    on_select: EventHandler<String>,
) -> Element {
    let is_current = crumb.path == current_path;
    let path = crumb.path.clone();

    rsx! {
        if is_current {
            span { class: "rounded-md bg-slate-100 px-2 py-1 font-semibold text-slate-800", "{crumb.label}" }
        } else {
            button {
                class: "rounded-md px-2 py-1 font-semibold text-emerald-800 hover:bg-emerald-50",
                onclick: move |_| on_select.call(path.clone()),
                "{crumb.label}"
            }
        }
    }
}

#[component]
fn BuilderMediaPickerEntry(
    entry: MediaEntry,
    selected_path: String,
    on_open_folder: EventHandler<String>,
    on_select_image: EventHandler<String>,
) -> Element {
    match entry.kind {
        MediaEntryKind::Folder => {
            let path = entry.path.clone();
            rsx! {
                button {
                    class: "grid gap-2 rounded-md border border-stone-200 bg-white p-2 text-left hover:border-emerald-700",
                    onclick: move |_| on_open_folder.call(path.clone()),
                    div { class: "flex h-24 items-center justify-center rounded-md bg-stone-100 text-xs font-semibold text-slate-600",
                        "Folder"
                    }
                    span { class: "truncate text-xs font-semibold text-slate-800", "{entry.name}" }
                }
            }
        }
        MediaEntryKind::Image => {
            let path = entry.path.clone();
            let url = entry.url.unwrap_or_else(|| format!("/media/{path}"));
            let class = if selected_path == path {
                "grid gap-2 rounded-md border border-emerald-700 bg-emerald-50 p-2 text-left"
            } else {
                "grid gap-2 rounded-md border border-stone-200 bg-white p-2 text-left hover:border-emerald-700"
            };

            rsx! {
                button {
                    class,
                    onclick: move |_| on_select_image.call(path.clone()),
                    div { class: "overflow-hidden rounded-md bg-stone-100",
                        img {
                            class: "h-24 w-full object-cover",
                            src: "{url}",
                            alt: "{entry.name}"
                        }
                    }
                    span { class: "truncate text-xs font-semibold text-slate-800", "{entry.name}" }
                }
            }
        }
        MediaEntryKind::Video => rsx! {},
    }
}

fn builder_media_breadcrumbs(path: &str) -> Vec<BuilderMediaCrumb> {
    let mut crumbs = vec![BuilderMediaCrumb {
        label: "Media".to_owned(),
        path: String::new(),
    }];
    let mut current = Vec::new();
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        current.push(segment);
        crumbs.push(BuilderMediaCrumb {
            label: segment.to_owned(),
            path: current.join("/"),
        });
    }
    crumbs
}

fn builder_media_preview_src(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else if value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("/media/")
        || value.starts_with("/assets/")
    {
        Some(value.to_owned())
    } else {
        Some(format!("/media/{}", value.trim_start_matches('/')))
    }
}

fn media_entry_is_builder_picker_option(entry: &MediaEntry) -> bool {
    matches!(entry.kind, MediaEntryKind::Folder | MediaEntryKind::Image)
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
    fn markdown_block_from_builder_draft_should_create_infobox_fence() {
        let mut draft = draft(BuilderBlockKind::Infobox);
        draft.title = "Nikola Tesla".to_owned();
        draft.image = "portraits/tesla.jpg".to_owned();
        draft.alt = "Nikola Tesla portrait".to_owned();
        draft.caption = "Tesla around 1890".to_owned();
        draft.rows = "Born: 10 July 1856".to_owned();

        let markdown = markdown_block_from_builder_draft(&draft);

        assert_eq!(
            markdown,
            "```infobox\ntitle: Nikola Tesla\nimage: portraits/tesla.jpg\nalt: Nikola Tesla portrait\ncaption: Tesla around 1890\nBorn: 10 July 1856\n```"
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

    #[test]
    fn builder_media_preview_src_should_map_bare_paths_to_media_route() {
        let src = builder_media_preview_src("portraits/veyra.webp");

        assert_eq!(src.as_deref(), Some("/media/portraits/veyra.webp"));
    }

    #[test]
    fn builder_media_breadcrumbs_should_include_nested_media_paths() {
        let crumbs = builder_media_breadcrumbs("npc/portraits");

        assert_eq!(
            crumbs,
            vec![
                BuilderMediaCrumb {
                    label: "Media".to_owned(),
                    path: String::new(),
                },
                BuilderMediaCrumb {
                    label: "npc".to_owned(),
                    path: "npc".to_owned(),
                },
                BuilderMediaCrumb {
                    label: "portraits".to_owned(),
                    path: "npc/portraits".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn media_entry_is_builder_picker_option_should_exclude_videos() {
        let entry = MediaEntry {
            name: "intro.mp4".to_owned(),
            path: "intro.mp4".to_owned(),
            kind: MediaEntryKind::Video,
            size: Some(10),
            url: Some("/media/intro.mp4".to_owned()),
        };

        assert!(!media_entry_is_builder_picker_option(&entry));
    }
}
