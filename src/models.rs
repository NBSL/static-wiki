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
    pub categories: Vec<String>,
    pub promoted: bool,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub updated_by: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageDetail {
    pub slug: String,
    pub title: String,
    pub markdown: String,
    pub rendered_markdown: String,
    pub categories: Vec<String>,
    pub promoted: bool,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub updated_by: Option<String>,
}

impl From<&PageDetail> for PageSummary {
    fn from(page: &PageDetail) -> Self {
        Self {
            slug: page.slug.clone(),
            title: page.title.clone(),
            categories: page.categories.clone(),
            promoted: page.promoted,
            created_at: page.created_at,
            updated_at: page.updated_at,
            updated_by: page.updated_by.clone(),
        }
    }
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
pub struct HtmlExport {
    pub path: String,
    pub url: String,
    pub page_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SettingsOverview {
    pub current_role: String,
    pub data_dir: String,
    pub pages_dir: String,
    pub templates_dir: String,
    pub components_dir: String,
    pub media_dir: String,
    pub export_dir: String,
    pub role_file: String,
    pub user_file: String,
    pub session_file: String,
    pub default_role: String,
    pub media_upload_limit: String,
    pub configured_auth_providers: Vec<AuthProviderInfo>,
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
