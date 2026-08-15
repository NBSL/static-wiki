use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AuthProviderInfo {
    pub slug: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageSummary {
    pub slug: String,
    pub title: String,
    pub updated_at: Option<i64>,
    pub updated_by: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageDetail {
    pub slug: String,
    pub title: String,
    pub markdown: String,
    pub updated_at: Option<i64>,
    pub updated_by: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageTemplateSummary {
    pub slug: String,
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageTemplateDraft {
    pub template_slug: String,
    pub title: String,
    pub markdown: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MediaEntryKind {
    Folder,
    Image,
    Video,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MediaEntry {
    pub name: String,
    pub path: String,
    pub kind: MediaEntryKind,
    pub size: Option<u64>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MediaListing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<MediaEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageRevision {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author: String,
    pub timestamp: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
    Hunk,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageDiff {
    pub slug: String,
    pub revision: PageRevision,
    pub lines: Vec<DiffLine>,
}
