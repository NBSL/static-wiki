use markdown_it::{
    parser::{core::CoreRule, extset::MarkdownItExt},
    plugins::cmark::block::fence::CodeFence,
    MarkdownIt, Node, NodeValue, Renderer,
};
use serde::Deserialize;

const BUILTIN_MANIFESTS: &[&str] = &[
    include_str!("declarative/callout.json"),
    include_str!("declarative/infobox.json"),
    include_str!("declarative/item_card.json"),
];

#[derive(Clone, Debug)]
struct DeclarativeComponents {
    components: Vec<ComponentDefinition>,
}

impl MarkdownItExt for DeclarativeComponents {}

#[derive(Clone, Debug)]
struct ComponentDefinition {
    name: String,
    fences: Vec<String>,
    wrapper_tag: String,
    wrapper_class: Option<String>,
    aria_label: Option<String>,
    layout: Vec<LayoutNode>,
    fields: Vec<FieldDefinition>,
    unknown_fields: Option<UnknownFieldsDefinition>,
}

#[derive(Clone, Debug)]
enum LayoutNode {
    Container {
        tag: String,
        class: Option<String>,
        aria_hidden: bool,
        children: Vec<LayoutNode>,
    },
    Field {
        key: String,
    },
    RemainingFields,
}

#[derive(Clone, Debug)]
struct FieldDefinition {
    keys: Vec<String>,
    kind: FieldKind,
    tag: String,
    class: Option<String>,
    class_prefix: Option<String>,
    wrapper_tag: String,
    wrapper_class: Option<String>,
    item_tag: String,
    item_class: Option<String>,
    pair_class: Option<String>,
    label_class: Option<String>,
    value_class: Option<String>,
    split: String,
    alt_from: Option<String>,
    default_value: Option<String>,
    aria_hidden: bool,
    repeatable: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FieldKind {
    ClassMarker,
    Text,
    Image,
    List,
    KeyValueList,
}

#[derive(Clone, Debug)]
struct UnknownFieldsDefinition {
    kind: UnknownFieldsKind,
    wrapper_tag: String,
    class: Option<String>,
    row_class: Option<String>,
    pair_class: Option<String>,
    label_class: Option<String>,
    value_class: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum UnknownFieldsKind {
    DefinitionList,
    KeyValueLines,
}

#[derive(Clone, Debug)]
struct ComponentInputField {
    label: String,
    key: String,
    value: String,
}

#[derive(Debug)]
struct DeclarativeComponent {
    definition: ComponentDefinition,
    fields: Vec<ComponentInputField>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct ComponentManifest {
    name: String,
    fence: String,
    aliases: Vec<String>,
    wrapper_tag: String,
    wrapper_class: Option<String>,
    aria_label: Option<String>,
    layout: Vec<LayoutManifest>,
    fields: Vec<FieldManifest>,
    unknown_fields: Option<UnknownFieldsManifest>,
}

impl Default for ComponentManifest {
    fn default() -> Self {
        Self {
            name: String::new(),
            fence: String::new(),
            aliases: Vec::new(),
            wrapper_tag: "div".to_owned(),
            wrapper_class: None,
            aria_label: None,
            layout: Vec::new(),
            fields: Vec::new(),
            unknown_fields: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct LayoutManifest {
    #[serde(rename = "type")]
    kind: Option<LayoutKind>,
    field: Option<String>,
    tag: String,
    class: Option<String>,
    aria_hidden: bool,
    children: Vec<LayoutManifest>,
}

impl Default for LayoutManifest {
    fn default() -> Self {
        Self {
            kind: None,
            field: None,
            tag: "div".to_owned(),
            class: None,
            aria_hidden: false,
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LayoutKind {
    Container,
    Field,
    RemainingFields,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct FieldManifest {
    key: String,
    aliases: Vec<String>,
    #[serde(rename = "type")]
    kind: Option<FieldKind>,
    tag: String,
    class: Option<String>,
    class_prefix: Option<String>,
    wrapper_tag: String,
    wrapper_class: Option<String>,
    item_tag: String,
    item_class: Option<String>,
    pair_class: Option<String>,
    label_class: Option<String>,
    value_class: Option<String>,
    split: String,
    alt_from: Option<String>,
    #[serde(rename = "default")]
    default_value: Option<String>,
    aria_hidden: bool,
    repeatable: bool,
}

impl Default for FieldManifest {
    fn default() -> Self {
        Self {
            key: String::new(),
            aliases: Vec::new(),
            kind: None,
            tag: "div".to_owned(),
            class: None,
            class_prefix: None,
            wrapper_tag: "div".to_owned(),
            wrapper_class: None,
            item_tag: "span".to_owned(),
            item_class: None,
            pair_class: None,
            label_class: None,
            value_class: None,
            split: ",".to_owned(),
            alt_from: None,
            default_value: None,
            aria_hidden: false,
            repeatable: false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct UnknownFieldsManifest {
    #[serde(rename = "type")]
    kind: UnknownFieldsKind,
    wrapper_tag: String,
    class: Option<String>,
    row_class: Option<String>,
    pair_class: Option<String>,
    label_class: Option<String>,
    value_class: Option<String>,
}

impl Default for UnknownFieldsManifest {
    fn default() -> Self {
        Self {
            kind: UnknownFieldsKind::DefinitionList,
            wrapper_tag: "dl".to_owned(),
            class: None,
            row_class: None,
            pair_class: None,
            label_class: None,
            value_class: None,
        }
    }
}

pub(super) fn add_rules(parser: &mut MarkdownIt, extra_manifests: &[String]) {
    let components = load_components(extra_manifests);
    if components.is_empty() {
        return;
    }

    parser.ext.insert(DeclarativeComponents { components });
    parser.add_rule::<DeclarativeComponentRule>();
}

fn load_components(extra_manifests: &[String]) -> Vec<ComponentDefinition> {
    BUILTIN_MANIFESTS
        .iter()
        .copied()
        .chain(extra_manifests.iter().map(String::as_str))
        .filter_map(parse_component_definition)
        .collect()
}

fn parse_component_definition(manifest: &str) -> Option<ComponentDefinition> {
    let manifest = serde_json::from_str::<ComponentManifest>(manifest).ok()?;
    ComponentDefinition::from_manifest(manifest)
}

impl ComponentDefinition {
    fn from_manifest(manifest: ComponentManifest) -> Option<Self> {
        let name = manifest.name.trim().to_owned();
        let fence = normalize_component_key(&manifest.fence)?;
        if name.is_empty() {
            return None;
        }

        let mut fences = vec![fence];
        fences.extend(
            manifest
                .aliases
                .iter()
                .filter_map(|alias| normalize_component_key(alias)),
        );
        fences.sort();
        fences.dedup();

        Some(Self {
            name,
            fences,
            wrapper_tag: safe_tag(&manifest.wrapper_tag, "div"),
            wrapper_class: safe_class_list(manifest.wrapper_class.as_deref()),
            aria_label: manifest.aria_label.filter(|label| !label.trim().is_empty()),
            layout: manifest
                .layout
                .into_iter()
                .filter_map(LayoutNode::from_manifest)
                .collect(),
            fields: manifest
                .fields
                .into_iter()
                .filter_map(FieldDefinition::from_manifest)
                .collect(),
            unknown_fields: manifest
                .unknown_fields
                .map(UnknownFieldsDefinition::from_manifest),
        })
    }

    fn matches_fence(&self, info: &str) -> bool {
        let Some(fence) = info
            .split_whitespace()
            .next()
            .and_then(normalize_component_key)
        else {
            return false;
        };

        self.fences.iter().any(|candidate| candidate == &fence)
    }

    fn field_by_key(&self, key: &str) -> Option<&FieldDefinition> {
        let key = normalize_component_key(key)?;
        self.fields
            .iter()
            .find(|definition| definition.keys.iter().any(|candidate| candidate == &key))
    }

    fn field_for_input(&self, field: &ComponentInputField) -> Option<&FieldDefinition> {
        self.fields
            .iter()
            .find(|definition| definition.matches_field(field))
    }
}

impl LayoutNode {
    fn from_manifest(manifest: LayoutManifest) -> Option<Self> {
        match manifest.kind? {
            LayoutKind::Container => Some(Self::Container {
                tag: safe_tag(&manifest.tag, "div"),
                class: safe_class_list(manifest.class.as_deref()),
                aria_hidden: manifest.aria_hidden,
                children: manifest
                    .children
                    .into_iter()
                    .filter_map(Self::from_manifest)
                    .collect(),
            }),
            LayoutKind::Field => Some(Self::Field {
                key: manifest
                    .field
                    .as_deref()
                    .and_then(normalize_component_key)?,
            }),
            LayoutKind::RemainingFields => Some(Self::RemainingFields),
        }
    }
}

impl FieldDefinition {
    fn from_manifest(manifest: FieldManifest) -> Option<Self> {
        let key = normalize_component_key(&manifest.key)?;
        let kind = manifest.kind?;
        let mut keys = vec![key];
        keys.extend(
            manifest
                .aliases
                .iter()
                .filter_map(|alias| normalize_component_key(alias)),
        );
        keys.sort();
        keys.dedup();

        Some(Self {
            keys,
            kind,
            tag: safe_tag(&manifest.tag, default_field_tag(kind)),
            class: safe_class_list(manifest.class.as_deref()),
            class_prefix: safe_class_prefix(manifest.class_prefix.as_deref()),
            wrapper_tag: safe_tag(&manifest.wrapper_tag, default_wrapper_tag(kind)),
            wrapper_class: safe_class_list(manifest.wrapper_class.as_deref()),
            item_tag: safe_tag(&manifest.item_tag, default_item_tag(kind)),
            item_class: safe_class_list(manifest.item_class.as_deref()),
            pair_class: safe_class_list(manifest.pair_class.as_deref()),
            label_class: safe_class_list(manifest.label_class.as_deref()),
            value_class: safe_class_list(manifest.value_class.as_deref()),
            split: non_empty_or_default(&manifest.split, ","),
            alt_from: manifest
                .alt_from
                .as_deref()
                .and_then(normalize_component_key),
            default_value: manifest
                .default_value
                .filter(|value| !value.trim().is_empty()),
            aria_hidden: manifest.aria_hidden,
            repeatable: manifest.repeatable,
        })
    }

    fn matches_field(&self, field: &ComponentInputField) -> bool {
        self.keys.iter().any(|key| key == &field.key)
    }
}

impl UnknownFieldsDefinition {
    fn from_manifest(manifest: UnknownFieldsManifest) -> Self {
        Self {
            kind: manifest.kind,
            wrapper_tag: safe_tag(&manifest.wrapper_tag, "dl"),
            class: safe_class_list(manifest.class.as_deref()),
            row_class: safe_class_list(manifest.row_class.as_deref()),
            pair_class: safe_class_list(manifest.pair_class.as_deref()),
            label_class: safe_class_list(manifest.label_class.as_deref()),
            value_class: safe_class_list(manifest.value_class.as_deref()),
        }
    }
}

impl NodeValue for DeclarativeComponent {
    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
        fmt.open(
            &self.definition.wrapper_tag,
            &component_attrs(&self.definition),
        );

        let mut consumed = vec![false; self.fields.len()];
        if self.definition.layout.is_empty() {
            for definition in &self.definition.fields {
                render_declared_field(definition, &self.fields, &mut consumed, true, fmt);
            }
        } else {
            for node in &self.definition.layout {
                render_layout_node(node, &self.definition, &self.fields, &mut consumed, fmt);
            }
        }

        if let Some(unknown_fields) = &self.definition.unknown_fields {
            render_unknown_fields(unknown_fields, &self.fields, &consumed, fmt);
        }

        fmt.close(&self.definition.wrapper_tag);
        fmt.cr();
    }
}

pub(super) struct DeclarativeComponentRule;

impl CoreRule for DeclarativeComponentRule {
    fn run(root: &mut Node, parser: &MarkdownIt) {
        let Some(components) = parser.ext.get::<DeclarativeComponents>() else {
            return;
        };

        root.walk_mut(|node, _| {
            let Some(fence) = node.cast::<CodeFence>() else {
                return;
            };

            let Some(definition) = components
                .components
                .iter()
                .rev()
                .find(|definition| definition.matches_fence(&fence.info))
                .cloned()
            else {
                return;
            };

            let fields = parse_component_input(&fence.content);
            node.replace(DeclarativeComponent { definition, fields });
        });
    }
}

fn render_layout_node(
    node: &LayoutNode,
    definition: &ComponentDefinition,
    fields: &[ComponentInputField],
    consumed: &mut [bool],
    fmt: &mut dyn Renderer,
) {
    match node {
        LayoutNode::Container {
            tag,
            class,
            aria_hidden,
            children,
        } => {
            fmt.open(tag, &layout_attrs(class, *aria_hidden));
            for child in children {
                render_layout_node(child, definition, fields, consumed, fmt);
            }
            fmt.close(tag);
        }
        LayoutNode::Field { key } => {
            if let Some(field_definition) = definition.field_by_key(key) {
                render_declared_field(field_definition, fields, consumed, true, fmt);
            }
        }
        LayoutNode::RemainingFields => {
            render_remaining_fields(definition, fields, consumed, fmt);
        }
    }
}

fn render_remaining_fields(
    definition: &ComponentDefinition,
    fields: &[ComponentInputField],
    consumed: &mut [bool],
    fmt: &mut dyn Renderer,
) {
    for index in 0..fields.len() {
        if consumed[index] {
            continue;
        }

        if let Some(field_definition) = definition.field_for_input(&fields[index]) {
            render_field_at_index(field_definition, fields, index, consumed, fmt);
        } else if let Some(unknown_fields) = &definition.unknown_fields {
            if render_unknown_field(unknown_fields, &fields[index], fmt) {
                consumed[index] = true;
            }
        }
    }
}

fn parse_component_input(content: &str) -> Vec<ComponentInputField> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let (label, value) = line.split_once(':')?;
            let label = label.trim();
            let value = value.trim();
            if label.is_empty() || value.is_empty() {
                return None;
            }

            Some(ComponentInputField {
                label: label.to_owned(),
                key: normalize_component_key(label)?,
                value: value.to_owned(),
            })
        })
        .collect()
}

fn render_declared_field(
    definition: &FieldDefinition,
    fields: &[ComponentInputField],
    consumed: &mut [bool],
    render_default: bool,
    fmt: &mut dyn Renderer,
) {
    let mut matching_indices = fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| {
            (!consumed[index] && definition.matches_field(field)).then_some(index)
        })
        .collect::<Vec<_>>();

    if matching_indices.is_empty() {
        if render_default {
            render_default_field(definition, fmt);
        }
        return;
    }

    if !definition.repeatable {
        matching_indices.truncate(1);
    }

    for index in matching_indices {
        render_field_at_index(definition, fields, index, consumed, fmt);
    }

    if !definition.repeatable {
        for index in fields
            .iter()
            .enumerate()
            .filter_map(|(index, field)| {
                (!consumed[index] && definition.matches_field(field)).then_some(index)
            })
            .collect::<Vec<_>>()
        {
            consumed[index] = true;
        }
    }
}

fn render_field_at_index(
    definition: &FieldDefinition,
    fields: &[ComponentInputField],
    index: usize,
    consumed: &mut [bool],
    fmt: &mut dyn Renderer,
) {
    consumed[index] = true;
    match definition.kind {
        FieldKind::ClassMarker => render_class_marker_field(definition, &fields[index].value, fmt),
        FieldKind::Text => render_text_field(definition, &fields[index].value, fmt),
        FieldKind::Image => {
            render_image_field(definition, &fields[index].value, fields, consumed, fmt)
        }
        FieldKind::List => render_list_field(definition, &fields[index].value, fmt),
        FieldKind::KeyValueList => {
            render_key_value_list_field(definition, &fields[index].value, fmt)
        }
    }
}

fn render_default_field(definition: &FieldDefinition, fmt: &mut dyn Renderer) {
    let Some(value) = &definition.default_value else {
        return;
    };

    match definition.kind {
        FieldKind::ClassMarker => render_class_marker_field(definition, value, fmt),
        FieldKind::Text => render_text_field(definition, value, fmt),
        FieldKind::Image => {
            let mut consumed = [];
            render_image_field(definition, value, &[], &mut consumed, fmt);
        }
        FieldKind::List => render_list_field(definition, value, fmt),
        FieldKind::KeyValueList => render_key_value_list_field(definition, value, fmt),
    }
}

fn render_class_marker_field(definition: &FieldDefinition, value: &str, fmt: &mut dyn Renderer) {
    let suffix = dynamic_class_suffix(value)
        .or_else(|| {
            definition
                .default_value
                .as_deref()
                .and_then(dynamic_class_suffix)
        })
        .unwrap_or_else(|| "generic".to_owned());
    let mut attrs = class_attrs(&definition.class);
    if let Some(prefix) = &definition.class_prefix {
        attrs.push(("class", format!("{prefix}{suffix}")));
    }
    if definition.aria_hidden {
        attrs.push(("aria-hidden", "true".to_owned()));
    }
    fmt.self_close(&definition.tag, &attrs);
}

fn render_text_field(definition: &FieldDefinition, value: &str, fmt: &mut dyn Renderer) {
    fmt.open(&definition.tag, &class_attrs(&definition.class));
    fmt.text(value);
    fmt.close(&definition.tag);
}

fn render_image_field(
    definition: &FieldDefinition,
    value: &str,
    fields: &[ComponentInputField],
    consumed: &mut [bool],
    fmt: &mut dyn Renderer,
) {
    let alt = definition
        .alt_from
        .as_ref()
        .and_then(|alt_key| {
            fields
                .iter()
                .enumerate()
                .find(|(_, field)| &field.key == alt_key)
                .map(|(index, field)| {
                    consumed[index] = true;
                    field.value.clone()
                })
        })
        .unwrap_or_default();

    let Some(src) = component_image_src(value) else {
        return;
    };

    let mut attrs = class_attrs(&definition.class);
    attrs.push(("src", src));
    attrs.push(("alt", alt));
    fmt.self_close("img", &attrs);
}

fn render_list_field(definition: &FieldDefinition, value: &str, fmt: &mut dyn Renderer) {
    let items = split_field_value(value, &definition.split).collect::<Vec<_>>();
    if items.is_empty() {
        return;
    }

    fmt.open(
        &definition.wrapper_tag,
        &class_attrs(&definition.wrapper_class),
    );
    for item in items {
        fmt.open(&definition.item_tag, &class_attrs(&definition.item_class));
        fmt.text(item);
        fmt.close(&definition.item_tag);
    }
    fmt.close(&definition.wrapper_tag);
}

fn render_key_value_list_field(definition: &FieldDefinition, value: &str, fmt: &mut dyn Renderer) {
    let pairs = split_field_value(value, &definition.split)
        .filter_map(|item| item.split_once(':'))
        .map(|(label, value)| (label.trim(), value.trim()))
        .filter(|(label, value)| !label.is_empty() && !value.is_empty())
        .collect::<Vec<_>>();
    if pairs.is_empty() {
        return;
    }

    fmt.open(
        &definition.wrapper_tag,
        &class_attrs(&definition.wrapper_class),
    );
    for (label, value) in pairs {
        fmt.open("span", &class_attrs(&definition.pair_class));
        fmt.open("span", &class_attrs(&definition.label_class));
        fmt.text(label);
        fmt.close("span");
        fmt.open("span", &class_attrs(&definition.value_class));
        fmt.text(value);
        fmt.close("span");
        fmt.close("span");
    }
    fmt.close(&definition.wrapper_tag);
}

fn render_unknown_fields(
    definition: &UnknownFieldsDefinition,
    fields: &[ComponentInputField],
    consumed: &[bool],
    fmt: &mut dyn Renderer,
) {
    match definition.kind {
        UnknownFieldsKind::DefinitionList => {
            render_unknown_definition_list(definition, fields, consumed, fmt);
        }
        UnknownFieldsKind::KeyValueLines => {
            for (index, field) in fields.iter().enumerate() {
                if !consumed[index] {
                    render_unknown_field(definition, field, fmt);
                }
            }
        }
    }
}

fn render_unknown_definition_list(
    definition: &UnknownFieldsDefinition,
    fields: &[ComponentInputField],
    consumed: &[bool],
    fmt: &mut dyn Renderer,
) {
    if fields.iter().enumerate().all(|(index, _)| consumed[index]) {
        return;
    }

    fmt.open(&definition.wrapper_tag, &class_attrs(&definition.class));
    for (index, field) in fields.iter().enumerate() {
        if consumed[index] {
            continue;
        }

        fmt.open("div", &class_attrs(&definition.row_class));
        fmt.open("dt", &class_attrs(&definition.label_class));
        fmt.text(&field.label);
        fmt.close("dt");
        fmt.open("dd", &class_attrs(&definition.value_class));
        fmt.text(&field.value);
        fmt.close("dd");
        fmt.close("div");
    }
    fmt.close(&definition.wrapper_tag);
}

fn render_unknown_field(
    definition: &UnknownFieldsDefinition,
    field: &ComponentInputField,
    fmt: &mut dyn Renderer,
) -> bool {
    if definition.kind != UnknownFieldsKind::KeyValueLines {
        return false;
    }

    fmt.open(&definition.wrapper_tag, &class_attrs(&definition.class));
    fmt.open("span", &class_attrs(&definition.pair_class));
    fmt.open("span", &class_attrs(&definition.label_class));
    fmt.text(&field.label);
    fmt.close("span");
    fmt.open("span", &class_attrs(&definition.value_class));
    fmt.text(&field.value);
    fmt.close("span");
    fmt.close("span");
    fmt.close(&definition.wrapper_tag);
    true
}

fn component_attrs(definition: &ComponentDefinition) -> Vec<(&'static str, String)> {
    let mut attrs = class_attrs(&definition.wrapper_class);
    attrs.push((
        "aria-label",
        definition
            .aria_label
            .clone()
            .unwrap_or_else(|| definition.name.clone()),
    ));
    attrs
}

fn class_attrs(class: &Option<String>) -> Vec<(&'static str, String)> {
    class
        .as_ref()
        .map(|class| vec![("class", class.clone())])
        .unwrap_or_default()
}

fn layout_attrs(class: &Option<String>, aria_hidden: bool) -> Vec<(&'static str, String)> {
    let mut attrs = class_attrs(class);
    if aria_hidden {
        attrs.push(("aria-hidden", "true".to_owned()));
    }
    attrs
}

fn split_field_value<'a>(value: &'a str, split: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    value
        .split(move |character| split.contains(character))
        .map(str::trim)
        .filter(|item| !item.is_empty())
}

fn normalize_component_key(value: &str) -> Option<String> {
    let mut key = String::new();
    let mut previous_was_separator = false;

    for character in value.trim().chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            key.push(character);
            previous_was_separator = false;
        } else if !previous_was_separator && !key.is_empty() {
            key.push('_');
            previous_was_separator = true;
        }
    }

    while key.ends_with('_') {
        key.pop();
    }

    (!key.is_empty()).then_some(key)
}

fn safe_tag(value: &str, fallback: &str) -> String {
    match value.trim() {
        "article" | "aside" | "dd" | "details" | "div" | "dl" | "dt" | "figcaption" | "figure"
        | "footer" | "header" | "li" | "nav" | "ol" | "p" | "section" | "span" | "strong"
        | "summary" | "ul" => value.trim().to_owned(),
        _ => fallback.to_owned(),
    }
}

fn safe_class_list(value: Option<&str>) -> Option<String> {
    let classes = value?
        .split_whitespace()
        .filter(|class| {
            !class.is_empty()
                && class.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                })
        })
        .collect::<Vec<_>>();

    (!classes.is_empty()).then(|| classes.join(" "))
}

fn safe_class_prefix(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_')))
    .then(|| value.to_owned())
}

fn dynamic_class_suffix(value: &str) -> Option<String> {
    let mut suffix = String::new();
    let mut previous_was_dash = false;
    for character in value.trim().chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            suffix.push(character);
            previous_was_dash = false;
        } else if !previous_was_dash && !suffix.is_empty() {
            suffix.push('-');
            previous_was_dash = true;
        }
    }

    while suffix.ends_with('-') {
        suffix.pop();
    }

    (!suffix.is_empty()).then_some(suffix)
}

fn default_field_tag(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::ClassMarker => "span",
        FieldKind::Text => "div",
        FieldKind::Image => "img",
        FieldKind::List | FieldKind::KeyValueList => "div",
    }
}

fn default_wrapper_tag(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::ClassMarker | FieldKind::Text | FieldKind::Image => "div",
        FieldKind::List => "ul",
        FieldKind::KeyValueList => "div",
    }
}

fn default_item_tag(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::List => "li",
        FieldKind::ClassMarker | FieldKind::Text | FieldKind::Image | FieldKind::KeyValueList => {
            "span"
        }
    }
}

fn non_empty_or_default(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}

fn component_image_src(src: &str) -> Option<String> {
    let src = src.trim();
    if src.is_empty() || src.starts_with("//") {
        return None;
    }

    if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("/media/") {
        return Some(src.to_owned());
    }

    if let Some(src) = src.strip_prefix("/assets/") {
        return local_media_src(src);
    }

    if let Some(src) = src.strip_prefix("./") {
        return local_media_src(src);
    }

    if src.starts_with('/') || src.starts_with("../") || src.contains(':') {
        return None;
    }

    local_media_src(src)
}

fn local_media_src(src: &str) -> Option<String> {
    let src = src.trim().trim_start_matches('/');
    (!src.is_empty()).then(|| format!("/media/{src}"))
}

#[cfg(test)]
mod tests {
    use crate::markdown::{render_markdown, render_markdown_with_component_manifests};

    #[test]
    fn render_markdown_should_render_builtin_declarative_callout() {
        let html = render_markdown(
            "```callout\ntitle: Note\nbody: This component is data-driven.\nitems: Fast | Safe\nStatus: Draft\n```",
        );

        assert!(html.contains(r#"<aside class="callout" aria-label="Callout">"#));
        assert!(html.contains(r#"<div class="callout-title">Note</div>"#));
        assert!(html.contains(r#"<li>Fast</li><li>Safe</li>"#));
        assert!(html.contains("<dt class=\"markdown-component-label\">Status</dt>"));
    }

    #[test]
    fn render_markdown_should_render_declarative_infobox_fence() {
        let html = render_markdown(
            "# Nikola Tesla\n\n```infobox\ntitle: Nikola Tesla\nBorn: 10 July 1856\nKnown for: AC power\n```\n\nBody",
        );

        assert!(html.contains(r#"<aside class="infobox" aria-label="Infobox">"#));
        assert!(html.contains(r#"<div class="infobox-title">Nikola Tesla</div>"#));
        assert!(html.contains("<dt>Born</dt><dd>10 July 1856</dd>"));
    }

    #[test]
    fn render_markdown_should_render_infocard_alias() {
        let html = render_markdown("```infocard\ntitle: Alias\nStatus: Draft\n```");

        assert!(html.contains(r#"<aside class="infobox" aria-label="Infobox">"#));
    }

    #[test]
    fn render_markdown_should_escape_declarative_infobox_values() {
        let html = render_markdown(
            "```infobox\ntitle: <script>alert('x')</script>\nBorn: <b>unsafe</b>\n```",
        );

        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;alert('x')&lt;/script&gt;"));
        assert!(html.contains("&lt;b&gt;unsafe&lt;/b&gt;"));
    }

    #[test]
    fn render_markdown_should_skip_unsafe_declarative_infobox_image_src() {
        let html = render_markdown("```infobox\ntitle: Example\nimage: javascript:alert(1)\n```");

        assert!(!html.contains("javascript:alert"));
    }

    #[test]
    fn render_markdown_should_map_bare_declarative_infobox_image_to_media_path() {
        let html = render_markdown("```infobox\ntitle: Example\nimage: tesla.jpeg\n```");

        assert!(html.contains(r#"src="/media/tesla.jpeg""#));
    }

    #[test]
    fn render_markdown_should_map_assets_declarative_infobox_image_to_media_path() {
        let html = render_markdown("```infobox\ntitle: Example\nimage: /assets/tesla.jpeg\n```");

        assert!(html.contains(r#"src="/media/tesla.jpeg""#));
    }

    #[test]
    fn render_markdown_should_render_declarative_item_card_fence() {
        let html = render_markdown(
            "```item-card\ntitle: Amulet of Loyalty\ntags: MAGIC, UNIQUE\nline: Slot: NECK\nline: INT: +5 | WIS: +5\nline: Mana: +30\ndescription: A simple pendant carrying Lady Elana's final vow.\nline: Weight: 0.1 | Size: SMALL\nClass: ALL\nRace: ALL\n```",
        );

        assert!(html.contains(r#"<figure class="item-card" aria-label="Item card">"#));
        assert!(
            html.contains(r#"<figcaption class="item-card-title">Amulet of Loyalty</figcaption>"#)
        );
        assert!(html.contains(r#"<span>MAGIC</span><span>UNIQUE</span>"#));
        assert!(html.contains(r#"class="item-card-icon-art item-card-icon-necklace""#));
        assert!(html.contains(
            r#"<span class="item-card-label">INT</span><span class="item-card-value">+5</span>"#
        ));
    }

    #[test]
    fn render_markdown_should_render_itembox_alias() {
        let html = render_markdown("```itembox\ntitle: Alias\nline: Slot: NECK\n```");

        assert!(html.contains(r#"<figure class="item-card" aria-label="Item card">"#));
    }

    #[test]
    fn render_markdown_should_escape_declarative_item_card_values() {
        let html = render_markdown(
            "```item-card\ntitle: <script>alert(1)</script>\nline: Class: <b>ALL</b>\n```",
        );

        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("&lt;b&gt;ALL&lt;/b&gt;"));
    }

    #[test]
    fn render_markdown_should_sanitize_declarative_item_icon_class() {
        let html = render_markdown("```item-card\ntitle: Example\nicon: weird/icon value!\n```");

        assert!(html.contains("item-card-icon-weird-icon-value"));
    }

    #[test]
    fn render_markdown_with_component_manifests_should_render_runtime_component() {
        let manifest = r#"{
            "name": "Badge",
            "fence": "badge",
            "wrapper_tag": "aside",
            "wrapper_class": "badge-card",
            "fields": [
                {
                    "key": "title",
                    "type": "text",
                    "tag": "strong",
                    "class": "badge-title"
                }
            ]
        }"#;

        let html = render_markdown_with_component_manifests(
            "```badge\ntitle: <script>alert(1)</script>\n```",
            &[manifest.to_owned()],
        );

        assert!(html.contains(r#"<aside class="badge-card" aria-label="Badge">"#));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn render_markdown_with_component_manifests_should_skip_unsafe_image_src() {
        let manifest = r#"{
            "name": "Image Box",
            "fence": "image-box",
            "fields": [
                {
                    "key": "image",
                    "type": "image",
                    "class": "image-box-image"
                }
            ]
        }"#;

        let html = render_markdown_with_component_manifests(
            "```image-box\nimage: javascript:alert(1)\n```",
            &[manifest.to_owned()],
        );

        assert!(!html.contains("javascript:alert"));
        assert!(!html.contains("<img"));
    }

    #[test]
    fn parse_component_definition_should_sanitize_manifest_markup() {
        let manifest = r#"{
            "name": "Unsafe",
            "fence": "unsafe",
            "wrapper_tag": "script",
            "wrapper_class": "safe-class bad/class",
            "fields": [
                {
                    "key": "title",
                    "type": "text",
                    "tag": "iframe",
                    "class": "field-class onclick=bad"
                }
            ]
        }"#;

        let definition =
            super::parse_component_definition(manifest).expect("component should parse");

        assert_eq!(definition.wrapper_tag, "div");
        assert_eq!(definition.wrapper_class.as_deref(), Some("safe-class"));
        assert_eq!(definition.fields[0].tag, "div");
        assert_eq!(definition.fields[0].class.as_deref(), Some("field-class"));
    }
}
