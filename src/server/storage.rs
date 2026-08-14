use crate::markdown::{humanize_slug, title_from_markdown};
use crate::models::{
    DiffLine, DiffLineKind, PageDetail, PageDiff, PageRevision, PageSummary, PageTemplateDraft,
    PageTemplateSummary,
};
use crate::slug::is_valid_slug;
use crate::user::AuthUser;
use git2::{
    Commit, DiffFormat, DiffOptions, IndexAddOption, Oid, Repository, RepositoryInitOptions,
    Signature, Tree,
};
use once_cell::sync::Lazy;
use std::fs;
use std::path::{Path, PathBuf};
use std::str;
use std::sync::Mutex;
use thiserror::Error;

static GIT_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

const PAGES_DIR: &str = "pages";
const TEMPLATES_DIR: &str = "templates";
const MEDIA_DIR: &str = "media";
const COMPONENTS_DIR: &str = "components";
const DEFAULT_HOME: &str = "# Home\n\nWelcome to your Rust and Dioxus wiki.\n";
const DEFAULT_COMPONENTS: [(&str, &str); 3] = [
    (
        "callout",
        include_str!("../markdown_components/declarative/callout.json"),
    ),
    (
        "infobox",
        include_str!("../markdown_components/declarative/infobox.json"),
    ),
    (
        "item-card",
        include_str!("../markdown_components/declarative/item_card.json"),
    ),
];
const DEFAULT_TEMPLATES: [(&str, &str); 3] = [
    (
        "article",
        "# Article\n\n```infobox\ntitle: {{title}}\nStatus: Draft\n```\n\nSummary for {{title}}.\n\n## Overview\n\n## Details\n\n## References\n",
    ),
    (
        "how-to",
        "# How-To\n\n## Goal\n\n## Steps\n\n1. \n\n## Verification\n",
    ),
    (
        "meeting-notes",
        "# Meeting Notes\n\n## Attendees\n\n## Notes\n\n## Decisions\n\n## Follow-ups\n",
    ),
];

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid page slug `{0}`")]
    InvalidSlug(String),
    #[error("revision `{0}` was not found")]
    RevisionNotFound(String),
    #[error("page template `{0}` was not found")]
    TemplateNotFound(String),
    #[error("history entry is not valid UTF-8")]
    Utf8(#[from] std::str::Utf8Error),
}

pub type StorageResult<T> = Result<T, StorageError>;

pub fn list_pages() -> StorageResult<Vec<PageSummary>> {
    with_store(|store| store.list_pages())
}

pub fn read_page(slug: &str) -> StorageResult<Option<PageDetail>> {
    with_store(|store| store.read_page(slug))
}

pub fn save_page(
    slug: &str,
    title: &str,
    markdown: &str,
    user: &AuthUser,
) -> StorageResult<PageDetail> {
    with_store(|store| store.save_page(slug, title, markdown, user))
}

pub fn list_templates() -> StorageResult<Vec<PageTemplateSummary>> {
    with_store(|store| store.list_templates())
}

pub fn list_component_manifests() -> StorageResult<Vec<String>> {
    with_store(|store| store.list_component_manifests())
}

pub fn page_template_draft(
    template_slug: &str,
    draft_slug: &str,
    title: &str,
) -> StorageResult<PageTemplateDraft> {
    with_store(|store| store.page_template_draft(template_slug, draft_slug, title))
}

pub fn page_history(slug: &str) -> StorageResult<Vec<PageRevision>> {
    with_store(|store| store.page_history(slug))
}

pub fn page_diff(slug: &str, revision: &str) -> StorageResult<PageDiff> {
    with_store(|store| store.page_diff(slug, revision))
}

pub fn media_dir() -> PathBuf {
    data_dir().join(MEDIA_DIR)
}

pub struct MediaFile {
    pub contents: Vec<u8>,
    pub content_type: &'static str,
}

pub fn read_media_file(request_path: &str) -> StorageResult<Option<MediaFile>> {
    read_media_file_from_roots(
        request_path,
        [
            media_dir(),
            PathBuf::from(MEDIA_DIR),
            PathBuf::from("assets"),
        ],
    )
}

fn read_media_file_from_roots(
    request_path: &str,
    roots: impl IntoIterator<Item = PathBuf>,
) -> StorageResult<Option<MediaFile>> {
    let Some(rel_path) = safe_media_rel_path(request_path) else {
        return Ok(None);
    };

    for root in roots {
        let path = root.join(&rel_path);
        if !path.is_file() {
            continue;
        }
        let Some(content_type) = media_content_type(&path) else {
            return Ok(None);
        };

        return Ok(Some(MediaFile {
            contents: fs::read(path)?,
            content_type,
        }));
    }

    Ok(None)
}

fn with_store<T>(operation: impl FnOnce(&WikiStore) -> StorageResult<T>) -> StorageResult<T> {
    let _guard = GIT_LOCK
        .lock()
        .map_err(|_| git2::Error::from_str("the wiki git lock was poisoned"))?;
    let store = WikiStore::open(data_dir())?;
    store.ensure_seed_page()?;
    store.ensure_seed_templates()?;
    store.ensure_seed_components()?;
    operation(&store)
}

fn data_dir() -> PathBuf {
    std::env::var_os("XP_WIKI_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("wiki-data"))
}

pub struct WikiStore {
    root: PathBuf,
    repo: Repository,
}

impl WikiStore {
    pub fn open(root: impl Into<PathBuf>) -> StorageResult<Self> {
        let root = root.into();
        fs::create_dir_all(root.join(PAGES_DIR))?;
        fs::create_dir_all(root.join(MEDIA_DIR))?;

        let repo = if root.join(".git").exists() {
            Repository::open(&root)?
        } else {
            let mut options = RepositoryInitOptions::new();
            options.initial_head("main");
            Repository::init_opts(&root, &options)?
        };

        Ok(Self { root, repo })
    }

    fn ensure_seed_page(&self) -> StorageResult<()> {
        let path = self.page_path("home")?;
        if path.exists() {
            return Ok(());
        }

        let system = AuthUser {
            id: "system".to_owned(),
            name: "Wiki System".to_owned(),
            email: Some("wiki@example.invalid".to_owned()),
        };
        self.save_page("home", "Home", DEFAULT_HOME, &system)?;
        Ok(())
    }

    fn ensure_seed_templates(&self) -> StorageResult<()> {
        let template_dir = self.root.join(TEMPLATES_DIR);
        if template_dir.exists() {
            return Ok(());
        }

        fs::create_dir_all(&template_dir)?;
        for (slug, markdown) in DEFAULT_TEMPLATES {
            fs::write(self.template_path(slug)?, markdown)?;
        }
        Ok(())
    }

    fn ensure_seed_components(&self) -> StorageResult<()> {
        let component_dir = self.root.join(COMPONENTS_DIR);
        fs::create_dir_all(&component_dir)?;
        for (slug, manifest) in DEFAULT_COMPONENTS {
            let path = self.component_path(slug)?;
            if !path.exists() {
                fs::write(path, manifest)?;
            }
        }
        Ok(())
    }

    pub fn list_pages(&self) -> StorageResult<Vec<PageSummary>> {
        let mut pages = Vec::new();
        let page_dir = self.root.join(PAGES_DIR);
        for entry in fs::read_dir(page_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let Some(slug) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if let Some(page) = self.read_page(slug)? {
                pages.push(PageSummary {
                    slug: page.slug,
                    title: page.title,
                    updated_at: page.updated_at,
                    updated_by: page.updated_by,
                });
            }
        }

        pages.sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()));
        Ok(pages)
    }

    pub fn list_templates(&self) -> StorageResult<Vec<PageTemplateSummary>> {
        let mut templates = Vec::new();
        let template_dir = self.root.join(TEMPLATES_DIR);
        if !template_dir.exists() {
            return Ok(templates);
        }

        for entry in fs::read_dir(template_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let Some(slug) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if !is_valid_slug(slug) {
                continue;
            }

            let markdown = fs::read_to_string(&path)?;
            templates.push(PageTemplateSummary {
                slug: slug.to_owned(),
                title: title_from_markdown(&markdown, slug),
            });
        }

        templates.sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()));
        Ok(templates)
    }

    pub fn list_component_manifests(&self) -> StorageResult<Vec<String>> {
        let mut manifests = Vec::new();
        let component_dir = self.root.join(COMPONENTS_DIR);
        if !component_dir.exists() {
            return Ok(manifests);
        }

        let mut paths = Vec::new();
        for entry in fs::read_dir(component_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Some(slug) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if is_valid_slug(slug) {
                paths.push(path);
            }
        }
        paths.sort();

        for path in paths {
            manifests.push(fs::read_to_string(path)?);
        }

        Ok(manifests)
    }

    pub fn page_template_draft(
        &self,
        template_slug: &str,
        draft_slug: &str,
        title: &str,
    ) -> StorageResult<PageTemplateDraft> {
        validate_slug(template_slug)?;
        let path = self.template_path(template_slug)?;
        if !path.exists() {
            return Err(StorageError::TemplateNotFound(template_slug.to_owned()));
        }

        let markdown = fs::read_to_string(path)?;
        let template_title = title_from_markdown(&markdown, template_slug);
        let title = draft_title(title, draft_slug, &template_title);
        let body = template_body_from_markdown(&markdown);
        let markdown = render_template_body(&body, &title, draft_slug);

        Ok(PageTemplateDraft {
            template_slug: template_slug.to_owned(),
            title,
            markdown,
        })
    }

    pub fn read_page(&self, slug: &str) -> StorageResult<Option<PageDetail>> {
        validate_slug(slug)?;
        let path = self.page_path(slug)?;
        if !path.exists() {
            return Ok(None);
        }

        let markdown = fs::read_to_string(path)?;
        let latest = self.page_history(slug)?.into_iter().next();
        Ok(Some(PageDetail {
            slug: slug.to_owned(),
            title: title_from_markdown(&markdown, slug),
            markdown,
            updated_at: latest.as_ref().map(|revision| revision.timestamp),
            updated_by: latest.map(|revision| revision.author),
        }))
    }

    pub fn save_page(
        &self,
        slug: &str,
        title: &str,
        markdown: &str,
        user: &AuthUser,
    ) -> StorageResult<PageDetail> {
        validate_slug(slug)?;
        fs::create_dir_all(self.root.join(PAGES_DIR))?;
        fs::write(self.page_path(slug)?, markdown)?;

        let mut index = self.repo.index()?;
        index.add_all([PAGES_DIR], IndexAddOption::DEFAULT, None)?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;
        let signature = git_signature(user)?;
        let message = format!("Update {}", title.trim().if_empty_then(slug));

        let parent = self.head_commit().ok();
        let parent_refs = parent.iter().collect::<Vec<_>>();
        self.repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            &message,
            &tree,
            &parent_refs,
        )?;

        self.read_page(slug)?
            .ok_or_else(|| StorageError::InvalidSlug(slug.to_owned()))
    }

    pub fn page_history(&self, slug: &str) -> StorageResult<Vec<PageRevision>> {
        validate_slug(slug)?;
        let rel_path = page_rel_path(slug);
        let mut revisions = Vec::new();
        let mut walk = self.repo.revwalk()?;

        if self.repo.head().is_err() {
            return Ok(revisions);
        }

        walk.push_head()?;
        walk.set_sorting(git2::Sort::TIME)?;

        for oid in walk {
            let commit = self.repo.find_commit(oid?)?;
            if self.commit_touches_path(&commit, &rel_path)? {
                revisions.push(revision_from_commit(&commit));
            }
        }

        Ok(revisions)
    }

    pub fn page_diff(&self, slug: &str, revision: &str) -> StorageResult<PageDiff> {
        validate_slug(slug)?;
        let oid = Oid::from_str(revision)
            .map_err(|_| StorageError::RevisionNotFound(revision.to_owned()))?;
        let commit = self
            .repo
            .find_commit(oid)
            .map_err(|_| StorageError::RevisionNotFound(revision.to_owned()))?;
        let rel_path = page_rel_path(slug);

        if !self.commit_touches_path(&commit, &rel_path)? {
            return Err(StorageError::RevisionNotFound(revision.to_owned()));
        }

        let tree = commit.tree()?;
        let old_tree = self.parent_tree(&commit)?;
        let mut options = DiffOptions::new();
        options.pathspec(rel_path);
        let diff =
            self.repo
                .diff_tree_to_tree(old_tree.as_ref(), Some(&tree), Some(&mut options))?;

        let mut lines = Vec::new();
        diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
            let kind = match line.origin() {
                '+' => DiffLineKind::Addition,
                '-' => DiffLineKind::Deletion,
                'H' => DiffLineKind::Hunk,
                _ => DiffLineKind::Context,
            };

            if matches!(line.origin(), 'F' | 'B') {
                return true;
            }

            let content = String::from_utf8_lossy(line.content())
                .trim_end_matches(['\r', '\n'])
                .to_owned();
            lines.push(DiffLine {
                kind,
                old_lineno: line.old_lineno(),
                new_lineno: line.new_lineno(),
                content,
            });
            true
        })?;

        Ok(PageDiff {
            slug: slug.to_owned(),
            revision: revision_from_commit(&commit),
            lines,
        })
    }

    fn head_commit(&self) -> Result<Commit<'_>, git2::Error> {
        self.repo.head()?.peel_to_commit()
    }

    fn parent_tree<'repo>(&self, commit: &Commit<'repo>) -> StorageResult<Option<Tree<'repo>>> {
        if commit.parent_count() == 0 {
            return Ok(None);
        }
        Ok(Some(commit.parent(0)?.tree()?))
    }

    fn commit_touches_path(&self, commit: &Commit<'_>, rel_path: &Path) -> StorageResult<bool> {
        let new_tree = commit.tree()?;
        if commit.parent_count() == 0 {
            return Ok(new_tree.get_path(rel_path).is_ok());
        }

        for parent_index in 0..commit.parent_count() {
            let parent = commit.parent(parent_index)?;
            let old_tree = parent.tree()?;
            let mut options = DiffOptions::new();
            options.pathspec(rel_path);
            let diff = self.repo.diff_tree_to_tree(
                Some(&old_tree),
                Some(&new_tree),
                Some(&mut options),
            )?;
            if diff.deltas().len() > 0 {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn page_path(&self, slug: &str) -> StorageResult<PathBuf> {
        validate_slug(slug)?;
        Ok(self.root.join(page_rel_path(slug)))
    }

    fn template_path(&self, slug: &str) -> StorageResult<PathBuf> {
        validate_slug(slug)?;
        Ok(self.root.join(template_rel_path(slug)))
    }

    fn component_path(&self, slug: &str) -> StorageResult<PathBuf> {
        validate_slug(slug)?;
        Ok(self.root.join(component_rel_path(slug)))
    }
}

fn validate_slug(slug: &str) -> StorageResult<()> {
    if is_valid_slug(slug) {
        Ok(())
    } else {
        Err(StorageError::InvalidSlug(slug.to_owned()))
    }
}

fn page_rel_path(slug: &str) -> PathBuf {
    PathBuf::from(PAGES_DIR).join(format!("{slug}.md"))
}

fn template_rel_path(slug: &str) -> PathBuf {
    PathBuf::from(TEMPLATES_DIR).join(format!("{slug}.md"))
}

fn component_rel_path(slug: &str) -> PathBuf {
    PathBuf::from(COMPONENTS_DIR).join(format!("{slug}.json"))
}

fn safe_media_rel_path(request_path: &str) -> Option<PathBuf> {
    let mut rel_path = PathBuf::new();
    for segment in request_path.split('/') {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.contains('\\')
            || segment.contains('\0')
        {
            return None;
        }
        rel_path.push(segment);
    }

    (!rel_path.as_os_str().is_empty()).then_some(rel_path)
}

fn media_content_type(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("avif") => Some("image/avif"),
        Some("gif") => Some("image/gif"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("png") => Some("image/png"),
        Some("svg") => Some("image/svg+xml"),
        Some("webp") => Some("image/webp"),
        _ => None,
    }
}

fn draft_title(title: &str, draft_slug: &str, template_title: &str) -> String {
    let title = title.trim();
    if !title.is_empty() {
        return title.to_owned();
    }

    let draft_slug = draft_slug.trim();
    if draft_slug.is_empty() {
        template_title.to_owned()
    } else {
        humanize_slug(draft_slug)
    }
}

fn template_body_from_markdown(markdown: &str) -> String {
    let mut removed_title = false;
    let mut body = Vec::new();

    for line in markdown.lines() {
        if !removed_title && line.strip_prefix("# ").is_some() {
            removed_title = true;
            continue;
        }

        if removed_title && body.is_empty() && line.trim().is_empty() {
            continue;
        }

        body.push(line);
    }

    body.join("\n").trim().to_owned()
}

fn render_template_body(body: &str, title: &str, slug: &str) -> String {
    body.replace("{{title}}", title.trim())
        .replace("{{slug}}", slug.trim())
}

fn git_signature(user: &AuthUser) -> StorageResult<Signature<'_>> {
    let email = user
        .email
        .as_deref()
        .filter(|email| email.contains('@'))
        .unwrap_or("wiki@example.invalid");
    Ok(Signature::now(&user.name, email)?)
}

fn revision_from_commit(commit: &Commit<'_>) -> PageRevision {
    let id = commit.id().to_string();
    let short_id = id.chars().take(8).collect();
    PageRevision {
        id,
        short_id,
        summary: commit.summary().unwrap_or("Wiki edit").to_owned(),
        author: commit.author().name().unwrap_or("Unknown").to_owned(),
        timestamp: commit.time().seconds(),
    }
}

trait IfEmpty {
    fn if_empty_then<'a>(&'a self, fallback: &'a str) -> &'a str;
}

impl IfEmpty for str {
    fn if_empty_then<'a>(&'a self, fallback: &'a str) -> &'a str {
        if self.is_empty() {
            fallback
        } else {
            self
        }
    }
}

#[allow(dead_code)]
fn fallback_title(slug: &str) -> String {
    humanize_slug(slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_user() -> AuthUser {
        AuthUser {
            id: "1".to_owned(),
            name: "Test User".to_owned(),
            email: Some("test@example.com".to_owned()),
        }
    }

    #[test]
    fn save_page_should_create_history_entry() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");

        store
            .save_page("guide", "Guide", "# Guide\n\nFirst", &test_user())
            .expect("page should save");
        let history = store.page_history("guide").expect("history should load");

        assert_eq!(history.len(), 1);
    }

    #[test]
    fn page_diff_should_include_git2_diff_lines() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");

        store
            .save_page("guide", "Guide", "# Guide\n\nFirst", &test_user())
            .expect("page should save");
        store
            .save_page("guide", "Guide", "# Guide\n\nSecond", &test_user())
            .expect("page should save");
        let revision = store.page_history("guide").expect("history should load")[0]
            .id
            .clone();
        let diff = store
            .page_diff("guide", &revision)
            .expect("diff should load");

        assert!(diff
            .lines
            .iter()
            .any(|line| line.kind == DiffLineKind::Addition));
    }

    #[test]
    fn list_templates_should_read_static_markdown_files() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");
        let template_dir = dir.path().join(TEMPLATES_DIR);
        fs::create_dir_all(&template_dir).expect("template dir should be created");
        fs::write(
            template_dir.join("release-notes.md"),
            "# Release Notes\n\n## Changes\n",
        )
        .expect("template should be written");

        let templates = store.list_templates().expect("templates should load");

        assert!(templates
            .iter()
            .any(|template| template.slug == "release-notes" && template.title == "Release Notes"));
    }

    #[test]
    fn list_component_manifests_should_read_seeded_static_json_files() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");

        store
            .ensure_seed_components()
            .expect("component manifests should be seeded");
        let manifests = store
            .list_component_manifests()
            .expect("component manifests should load");

        assert!(manifests
            .iter()
            .any(|manifest| manifest.contains(r#""fence": "callout""#)));
        assert!(manifests
            .iter()
            .any(|manifest| manifest.contains(r#""fence": "infobox""#)));
        assert!(manifests
            .iter()
            .any(|manifest| manifest.contains(r#""fence": "item-card""#)));
    }

    #[test]
    fn page_template_draft_should_strip_heading_and_replace_placeholders() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");
        let template_dir = dir.path().join(TEMPLATES_DIR);
        fs::create_dir_all(&template_dir).expect("template dir should be created");
        fs::write(
            template_dir.join("guide.md"),
            "# Guide\n\nIntro for {{title}} at {{slug}}.\n\n## Steps\n",
        )
        .expect("template should be written");

        let draft = store
            .page_template_draft("guide", "install-guide", "Install Guide")
            .expect("template draft should load");

        assert_eq!(
            draft.markdown,
            "Intro for Install Guide at install-guide.\n\n## Steps"
        );
    }

    #[test]
    fn safe_media_rel_path_should_reject_parent_segments() {
        let path = safe_media_rel_path("../roles.json");

        assert!(path.is_none());
    }

    #[test]
    fn read_media_file_from_roots_should_read_image_file() {
        let dir = tempdir().expect("temp dir should be created");
        fs::write(dir.path().join("example.svg"), "<svg></svg>")
            .expect("media file should be written");

        let media = read_media_file_from_roots("example.svg", [dir.path().to_path_buf()])
            .expect("media lookup should not fail")
            .expect("media file should be found");

        assert_eq!(media.content_type, "image/svg+xml");
    }
}
