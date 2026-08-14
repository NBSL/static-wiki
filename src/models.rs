use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AuthUser {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AuthProviderInfo {
    pub slug: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UserAccess {
    pub role: Option<String>,
    pub can_manage_users: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ManagedUser {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub role: String,
    pub locked: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ManagedUserInput {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub role: String,
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
