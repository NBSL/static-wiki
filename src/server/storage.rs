use crate::markdown::{
    humanize_slug, page_body_from_markdown, render_markdown_with_component_manifests,
    title_from_markdown,
};
use crate::models::{
    DiffLine, DiffLineKind, HtmlExport, MediaEntry, MediaEntryKind, MediaListing, PageDetail,
    PageDiff, PageRevision, PageSummary, PageTemplateDraft, PageTemplateSummary,
};
use crate::slug::{is_valid_slug, normalize_slug, validate_page_title};
use crate::user::AuthUser;
use git2::{
    Commit, DiffFormat, DiffOptions, IndexAddOption, Oid, Repository, RepositoryInitOptions,
    Signature, Tree,
};
use once_cell::sync::Lazy;
use std::fmt::Write as _;
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
const EXPORT_DIR: &str = "export";
const EXPORT_LATEST_DIR: &str = "latest";
pub(crate) const MEDIA_FILE_MAX_BYTES: usize = 50 * 1024 * 1024;
const EXPORT_TAILWIND_CSS: &str = include_str!("../../assets/tailwind.css");
const DEFAULT_HOME: &str = "# Home\n\nWelcome to your Rust and Dioxus wiki.\n";
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
    #[error("invalid page title: {0}")]
    InvalidTitle(String),
    #[error("revision `{0}` was not found")]
    RevisionNotFound(String),
    #[error("page template `{0}` was not found")]
    TemplateNotFound(String),
    #[error("invalid media path `{0}`")]
    InvalidMediaPath(String),
    #[error("media folder `{0}` was not found")]
    MediaFolderNotFound(String),
    #[error("media folder `{0}` already exists")]
    MediaFolderExists(String),
    #[error("media file `{0}` was not found")]
    MediaFileNotFound(String),
    #[error("media file `{0}` already exists")]
    MediaFileExists(String),
    #[error("media entry `{0}` was not found")]
    MediaEntryNotFound(String),
    #[error("media file is too large ({size} bytes, maximum {limit} bytes)")]
    MediaFileTooLarge { size: usize, limit: usize },
    #[error("unsupported media type `{0}`")]
    UnsupportedMediaType(String),
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

pub fn list_media(path: &str) -> StorageResult<MediaListing> {
    with_store(|store| store.list_media(path))
}

pub fn create_media_folder(
    parent: &str,
    name: &str,
    user: &AuthUser,
) -> StorageResult<MediaListing> {
    with_store(|store| store.create_media_folder(parent, name, user))
}

pub fn save_media_file(
    folder: &str,
    filename: &str,
    contents: &[u8],
    user: &AuthUser,
) -> StorageResult<MediaListing> {
    with_store(|store| store.save_media_file(folder, filename, contents, user))
}

pub fn move_media_file(
    source_path: &str,
    target_folder: &str,
    user: &AuthUser,
) -> StorageResult<()> {
    with_store(|store| store.move_media_file(source_path, target_folder, user))
}

pub fn delete_media_entry(path: &str, user: &AuthUser) -> StorageResult<()> {
    with_store(|store| store.delete_media_entry(path, user))
}

pub fn list_templates() -> StorageResult<Vec<PageTemplateSummary>> {
    with_store(|store| store.list_templates())
}

pub fn list_component_manifests() -> StorageResult<Vec<String>> {
    with_store(|store| store.list_component_manifests())
}

pub fn export_html_site() -> StorageResult<HtmlExport> {
    with_store(|store| store.export_html_site())
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

pub struct ExportFile {
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

pub fn read_export_file(request_path: &str) -> StorageResult<Option<ExportFile>> {
    read_export_file_from_root(request_path, data_dir().join(EXPORT_DIR))
}

fn read_export_file_from_root(
    request_path: &str,
    export_root: impl AsRef<Path>,
) -> StorageResult<Option<ExportFile>> {
    let Some(rel_path) = safe_export_rel_path(request_path) else {
        return Ok(None);
    };

    let mut path = export_root.as_ref().join(rel_path);
    if path.is_dir() {
        path = path.join("index.html");
    }
    if !path.is_file() {
        return Ok(None);
    }

    let content_type = export_content_type(&path);
    Ok(Some(ExportFile {
        contents: fs::read(path)?,
        content_type,
    }))
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
    crate::server::env::optional_env("XP_WIKI_DATA_DIR")
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
        for (slug, manifest) in crate::markdown_components::builtin_declarative_manifests() {
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
            let Some(slug) = valid_slug_from_path(&path, "md") else {
                continue;
            };
            let markdown = fs::read_to_string(&path)?;
            pages.push(self.page_summary_from_markdown(slug, &markdown)?);
        }

        sort_pages_by_creation_time(&mut pages);
        Ok(pages)
    }

    fn page_summary_from_markdown(&self, slug: &str, markdown: &str) -> StorageResult<PageSummary> {
        validate_slug(slug)?;
        let history = self.page_history(slug)?;
        let latest = history.first();
        Ok(PageSummary {
            slug: slug.to_owned(),
            title: title_from_markdown(markdown, slug),
            categories: page_categories_from_markdown(markdown),
            promoted: page_promoted_from_markdown(markdown),
            created_at: history.last().map(|revision| revision.timestamp),
            updated_at: latest.map(|revision| revision.timestamp),
            updated_by: latest.map(|revision| revision.author.clone()),
        })
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
            let Some(slug) = valid_slug_from_path(&path, "md") else {
                continue;
            };

            let markdown = fs::read_to_string(&path)?;
            templates.push(PageTemplateSummary {
                slug: slug.to_owned(),
                title: title_from_markdown(&markdown, slug),
            });
        }

        templates.sort_by_key(|template| template.title.to_lowercase());
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
            if valid_slug_from_path(&path, "json").is_some() {
                paths.push(path);
            }
        }
        paths.sort();

        for path in paths {
            manifests.push(fs::read_to_string(path)?);
        }

        Ok(manifests)
    }

    pub fn export_html_site(&self) -> StorageResult<HtmlExport> {
        let export_dir = self.root.join(EXPORT_DIR).join(EXPORT_LATEST_DIR);
        if export_dir.exists() {
            fs::remove_dir_all(&export_dir)?;
        }
        fs::create_dir_all(export_dir.join("assets"))?;
        fs::create_dir_all(export_dir.join("media"))?;
        fs::write(
            export_dir.join("assets").join("tailwind.css"),
            EXPORT_TAILWIND_CSS,
        )?;

        let pages = self.export_pages()?;
        let manifests = self.list_component_manifests()?;
        let page_summaries = page_summaries_from_details(&pages);
        for page in &pages {
            let html = render_export_page(page, &pages, &page_summaries, &manifests);
            fs::write(export_dir.join(export_page_file_name(&page.slug)), html)?;
        }

        let index_html = match pages
            .iter()
            .find(|page| page.slug == "home")
            .or_else(|| pages.first())
        {
            Some(page) => render_export_page(page, &pages, &page_summaries, &manifests),
            None => export_empty_index_html(),
        };
        fs::write(export_dir.join("index.html"), index_html)?;

        copy_export_media(&self.root, &export_dir)?;

        Ok(HtmlExport {
            path: export_dir.display().to_string(),
            url: format!("/exports/{EXPORT_LATEST_DIR}/index.html"),
            page_count: pages.len(),
        })
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
        let summary = self.page_summary_from_markdown(slug, &markdown)?;
        let category_pages = self.list_pages()?;
        let rendered_markdown =
            expand_category_shortcodes(page_body_from_markdown(&markdown), &category_pages);

        let PageSummary {
            slug,
            title,
            categories,
            promoted,
            created_at,
            updated_at,
            updated_by,
        } = summary;
        Ok(Some(PageDetail {
            slug,
            title,
            markdown,
            rendered_markdown,
            categories,
            promoted,
            created_at,
            updated_at,
            updated_by,
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
        validate_page_title(title)
            .map_err(|error| StorageError::InvalidTitle(error.to_string()))?;
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

    pub fn list_media(&self, path: &str) -> StorageResult<MediaListing> {
        let rel_path = safe_media_folder_rel_path(path)
            .ok_or_else(|| StorageError::InvalidMediaPath(path.to_owned()))?;
        let media_path = self.root.join(MEDIA_DIR).join(&rel_path);
        if !media_path.exists() {
            return Err(StorageError::MediaFolderNotFound(display_media_path(
                &rel_path,
            )));
        }
        if !media_path.is_dir() {
            return Err(StorageError::InvalidMediaPath(display_media_path(
                &rel_path,
            )));
        }

        let mut entries = Vec::new();
        for entry in fs::read_dir(media_path)? {
            let entry = entry?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name == ".gitkeep" {
                continue;
            }

            let entry_rel_path = rel_path.join(name);
            if path.is_dir() {
                entries.push(MediaEntry {
                    name: name.to_owned(),
                    path: media_path_string(&entry_rel_path),
                    kind: MediaEntryKind::Folder,
                    size: None,
                    url: None,
                });
                continue;
            }

            if !path.is_file() {
                continue;
            }

            let Some(kind) = media_kind_for_path(&path) else {
                continue;
            };
            entries.push(MediaEntry {
                name: name.to_owned(),
                path: media_path_string(&entry_rel_path),
                kind,
                size: Some(path.metadata()?.len()),
                url: Some(media_url(&entry_rel_path)),
            });
        }

        entries.sort_by(|left, right| {
            media_entry_rank(left.kind)
                .cmp(&media_entry_rank(right.kind))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });

        Ok(MediaListing {
            path: media_path_string(&rel_path),
            parent: media_parent_path(&rel_path),
            entries,
        })
    }

    pub fn create_media_folder(
        &self,
        parent: &str,
        name: &str,
        user: &AuthUser,
    ) -> StorageResult<MediaListing> {
        let parent_rel_path = safe_media_folder_rel_path(parent)
            .ok_or_else(|| StorageError::InvalidMediaPath(parent.to_owned()))?;
        let folder_name = normalize_slug(name)
            .ok_or_else(|| StorageError::InvalidMediaPath(name.trim().to_owned()))?;
        let rel_path = parent_rel_path.join(folder_name);
        let path = self.root.join(MEDIA_DIR).join(&rel_path);

        if path.exists() {
            return Err(StorageError::MediaFolderExists(display_media_path(
                &rel_path,
            )));
        }

        fs::create_dir_all(&path)?;
        fs::write(path.join(".gitkeep"), "")?;
        self.commit_media_change(
            &format!("Create media folder {}", display_media_path(&rel_path)),
            user,
        )?;
        self.list_media(parent)
    }

    pub fn save_media_file(
        &self,
        folder: &str,
        filename: &str,
        contents: &[u8],
        user: &AuthUser,
    ) -> StorageResult<MediaListing> {
        let folder_rel_path = safe_media_folder_rel_path(folder)
            .ok_or_else(|| StorageError::InvalidMediaPath(folder.to_owned()))?;
        let filename = safe_media_filename(filename)
            .ok_or_else(|| StorageError::InvalidMediaPath(filename.to_owned()))?;
        if media_content_type(Path::new(&filename)).is_none() {
            return Err(StorageError::UnsupportedMediaType(filename));
        }
        validate_media_file_size(contents.len())?;

        let folder_path = self.root.join(MEDIA_DIR).join(&folder_rel_path);
        if !folder_path.exists() {
            return Err(StorageError::MediaFolderNotFound(display_media_path(
                &folder_rel_path,
            )));
        }
        if !folder_path.is_dir() {
            return Err(StorageError::InvalidMediaPath(display_media_path(
                &folder_rel_path,
            )));
        }

        let rel_path = folder_rel_path.join(&filename);
        fs::write(folder_path.join(&filename), contents)?;
        self.commit_media_change(
            &format!("Upload media {}", display_media_path(&rel_path)),
            user,
        )?;
        self.list_media(folder)
    }

    pub fn move_media_file(
        &self,
        source_path: &str,
        target_folder: &str,
        user: &AuthUser,
    ) -> StorageResult<()> {
        let source_rel_path = safe_media_rel_path(source_path)
            .ok_or_else(|| StorageError::InvalidMediaPath(source_path.to_owned()))?;
        let target_rel_path = safe_media_folder_rel_path(target_folder)
            .ok_or_else(|| StorageError::InvalidMediaPath(target_folder.to_owned()))?;
        let source = self.root.join(MEDIA_DIR).join(&source_rel_path);
        if !source.is_file() {
            return Err(StorageError::MediaFileNotFound(display_media_path(
                &source_rel_path,
            )));
        }
        if media_content_type(&source).is_none() {
            return Err(StorageError::UnsupportedMediaType(display_media_path(
                &source_rel_path,
            )));
        }

        let target_dir = self.root.join(MEDIA_DIR).join(&target_rel_path);
        if !target_dir.exists() {
            return Err(StorageError::MediaFolderNotFound(display_media_path(
                &target_rel_path,
            )));
        }
        if !target_dir.is_dir() {
            return Err(StorageError::InvalidMediaPath(display_media_path(
                &target_rel_path,
            )));
        }

        let Some(filename) = source_rel_path.file_name() else {
            return Err(StorageError::InvalidMediaPath(source_path.to_owned()));
        };
        let destination_rel_path = target_rel_path.join(filename);
        let destination = self.root.join(MEDIA_DIR).join(&destination_rel_path);
        if source == destination {
            return Ok(());
        }
        if destination.exists() {
            return Err(StorageError::MediaFileExists(display_media_path(
                &destination_rel_path,
            )));
        }

        fs::rename(source, destination)?;
        self.commit_media_change(
            &format!(
                "Move media {} to {}",
                display_media_path(&source_rel_path),
                display_media_path(&target_rel_path)
            ),
            user,
        )?;

        Ok(())
    }

    pub fn delete_media_entry(&self, path: &str, user: &AuthUser) -> StorageResult<()> {
        let rel_path = safe_media_rel_path(path)
            .ok_or_else(|| StorageError::InvalidMediaPath(path.to_owned()))?;
        let full_path = self.root.join(MEDIA_DIR).join(&rel_path);
        let display_path = display_media_path(&rel_path);

        if full_path.is_file() {
            if media_content_type(&full_path).is_none() {
                return Err(StorageError::UnsupportedMediaType(display_path));
            }

            fs::remove_file(full_path)?;
            self.ensure_empty_media_folder_placeholder(&rel_path)?;
            self.commit_media_change(&format!("Delete media {display_path}"), user)?;
            return Ok(());
        }

        if full_path.is_dir() {
            fs::remove_dir_all(full_path)?;
            self.ensure_empty_media_folder_placeholder(&rel_path)?;
            self.commit_media_change(&format!("Delete media folder {display_path}"), user)?;
            return Ok(());
        }

        Err(StorageError::MediaEntryNotFound(display_path))
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

    fn commit_media_change(&self, message: &str, user: &AuthUser) -> StorageResult<()> {
        let mut index = self.repo.index()?;
        index.add_all([MEDIA_DIR], IndexAddOption::DEFAULT, None)?;
        index.update_all([MEDIA_DIR], None)?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;
        let signature = git_signature(user)?;
        let parent = self.head_commit().ok();
        let parent_refs = parent.iter().collect::<Vec<_>>();

        self.repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )?;

        Ok(())
    }

    fn ensure_empty_media_folder_placeholder(&self, deleted_rel_path: &Path) -> StorageResult<()> {
        let Some(parent_rel_path) = deleted_rel_path.parent() else {
            return Ok(());
        };
        let parent_path = self.root.join(MEDIA_DIR).join(parent_rel_path);
        if !parent_path.is_dir() || parent_path.read_dir()?.next().is_some() {
            return Ok(());
        }

        fs::write(parent_path.join(".gitkeep"), "")?;
        Ok(())
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

    fn export_pages(&self) -> StorageResult<Vec<PageDetail>> {
        let mut pages = Vec::new();
        for page in self.list_pages()? {
            if let Some(page) = self.read_page(&page.slug)? {
                pages.push(page);
            }
        }
        Ok(pages)
    }
}

fn validate_slug(slug: &str) -> StorageResult<()> {
    if is_valid_slug(slug) {
        Ok(())
    } else {
        Err(StorageError::InvalidSlug(slug.to_owned()))
    }
}

fn valid_slug_from_path<'a>(path: &'a Path, extension: &str) -> Option<&'a str> {
    if path.extension().and_then(|value| value.to_str()) != Some(extension) {
        return None;
    }

    let slug = path.file_stem()?.to_str()?;
    is_valid_slug(slug).then_some(slug)
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

fn sort_pages_by_creation_time(pages: &mut [PageSummary]) {
    pages.sort_by_cached_key(|page| {
        (
            page.created_at.unwrap_or(i64::MAX),
            page.title.to_lowercase(),
            page.slug.clone(),
        )
    });
}

fn page_categories_from_markdown(markdown: &str) -> Vec<String> {
    crate::markdown::category_slugs_from_markdown(markdown)
}

fn page_promoted_from_markdown(markdown: &str) -> bool {
    crate::markdown::promoted_from_page_markdown(markdown)
}

fn expand_category_shortcodes(markdown: &str, pages: &[PageSummary]) -> String {
    let mut expanded = String::with_capacity(markdown.len());
    let mut remaining = markdown;

    while let Some(start) = remaining.find("{{") {
        expanded.push_str(&remaining[..start]);
        let after_open = &remaining[start + 2..];
        let Some(end) = after_open.find("}}") else {
            expanded.push_str(&remaining[start..]);
            return expanded;
        };

        let token = &after_open[..end];
        if let Some(category) = category_slug_from_shortcode(token) {
            expanded.push_str(&category_page_list_markdown(&category, pages));
        } else {
            expanded.push_str(&remaining[start..start + 2 + end + 2]);
        }
        remaining = &after_open[end + 2..];
    }

    expanded.push_str(remaining);
    expanded
}

fn category_slug_from_shortcode(token: &str) -> Option<String> {
    let token = token.trim();
    if token.eq_ignore_ascii_case("title") || token.eq_ignore_ascii_case("slug") {
        return None;
    }

    if let Some(category) = token.strip_prefix("category:") {
        normalize_slug(category)
    } else if token.contains(':') {
        None
    } else {
        normalize_slug(token)
    }
}

fn category_page_list_markdown(category: &str, pages: &[PageSummary]) -> String {
    let entries = pages
        .iter()
        .filter(|page| {
            page.categories
                .iter()
                .any(|candidate| candidate == category)
        })
        .map(|page| format!("- [{}](/{})", escape_markdown_text(&page.title), page.slug))
        .collect::<Vec<_>>();

    if entries.is_empty() {
        return format!("_No pages in category `{category}`._");
    }

    entries.join("\n")
}

fn escape_markdown_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '\\' | '`'
                | '*'
                | '_'
                | '{'
                | '}'
                | '['
                | ']'
                | '('
                | ')'
                | '#'
                | '+'
                | '-'
                | '.'
                | '!'
                | '|'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn page_summaries_from_details(pages: &[PageDetail]) -> Vec<PageSummary> {
    pages.iter().map(PageSummary::from).collect()
}

fn export_page_file_name(slug: &str) -> String {
    if slug == "index" {
        "index-page.html".to_owned()
    } else {
        format!("{slug}.html")
    }
}

fn safe_export_rel_path(request_path: &str) -> Option<PathBuf> {
    let trimmed = request_path.trim_matches('/');
    if trimmed.is_empty() {
        return Some(PathBuf::from(EXPORT_LATEST_DIR).join("index.html"));
    }

    safe_relative_path(trimmed)
}

fn safe_media_folder_rel_path(request_path: &str) -> Option<PathBuf> {
    let trimmed = request_path.trim_matches('/');
    if trimmed.is_empty() {
        return Some(PathBuf::new());
    }

    safe_media_rel_path(trimmed)
}

fn safe_media_rel_path(request_path: &str) -> Option<PathBuf> {
    safe_relative_path(request_path)
}

fn safe_relative_path(request_path: &str) -> Option<PathBuf> {
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

fn safe_media_filename(filename: &str) -> Option<String> {
    let filename = filename.trim();
    if filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains('\0')
    {
        return None;
    }

    let path = Path::new(filename);
    let stem = path.file_stem()?.to_str()?;
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    media_content_type(Path::new(&format!("file.{extension}")))?;

    normalize_slug(stem).map(|stem| format!("{stem}.{extension}"))
}

fn validate_media_file_size(size: usize) -> StorageResult<()> {
    if size > MEDIA_FILE_MAX_BYTES {
        Err(StorageError::MediaFileTooLarge {
            size,
            limit: MEDIA_FILE_MAX_BYTES,
        })
    } else {
        Ok(())
    }
}

fn media_path_string(path: &Path) -> String {
    path.iter()
        .filter_map(|segment| segment.to_str())
        .collect::<Vec<_>>()
        .join("/")
}

fn media_parent_path(path: &Path) -> Option<String> {
    if path.as_os_str().is_empty() {
        return None;
    }

    let mut parent = path.to_path_buf();
    parent.pop();
    Some(media_path_string(&parent))
}

fn display_media_path(path: &Path) -> String {
    let path = media_path_string(path);
    if path.is_empty() {
        "/".to_owned()
    } else {
        path
    }
}

fn media_url(path: &Path) -> String {
    format!("/media/{}", media_path_string(path))
}

fn export_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        _ => media_content_type(path).unwrap_or("application/octet-stream"),
    }
}

fn media_kind_for_path(path: &Path) -> Option<MediaEntryKind> {
    let content_type = media_content_type(path)?;
    if content_type.starts_with("image/") {
        Some(MediaEntryKind::Image)
    } else if content_type.starts_with("video/") {
        Some(MediaEntryKind::Video)
    } else {
        None
    }
}

fn media_entry_rank(kind: MediaEntryKind) -> u8 {
    match kind {
        MediaEntryKind::Folder => 0,
        MediaEntryKind::Image => 1,
        MediaEntryKind::Video => 2,
    }
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
        Some("m4v") => Some("video/x-m4v"),
        Some("mov") => Some("video/quicktime"),
        Some("mp4") => Some("video/mp4"),
        Some("ogg" | "ogv") => Some("video/ogg"),
        Some("webm") => Some("video/webm"),
        _ => None,
    }
}

fn render_export_page(
    page: &PageDetail,
    pages: &[PageDetail],
    page_summaries: &[PageSummary],
    manifests: &[String],
) -> String {
    let expanded =
        expand_category_shortcodes(page_body_from_markdown(&page.markdown), page_summaries);
    let rendered = render_markdown_with_component_manifests(&expanded, manifests);
    export_page_html(page, pages, &rewrite_export_asset_paths(&rendered))
}

fn export_page_html(page: &PageDetail, pages: &[PageDetail], body_html: &str) -> String {
    let mut nav = String::new();
    for nav_page in pages {
        if !nav_page.promoted {
            continue;
        }

        let class = if nav_page.slug == page.slug {
            "flex min-h-10 w-full items-center justify-between gap-3 rounded-md border border-stone-300 bg-white px-3 text-left text-sm font-semibold text-slate-950"
        } else {
            "flex min-h-10 w-full items-center justify-between gap-3 rounded-md border border-transparent bg-transparent px-3 text-left text-sm font-medium text-slate-700 hover:bg-white"
        };
        let _ = write!(
            nav,
            r#"<a class="{class}" href="{}"><span class="truncate">{}</span></a>"#,
            escape_html_attr(&export_page_file_name(&nav_page.slug)),
            escape_html(&nav_page.title)
        );
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{}</title>
<link rel="stylesheet" href="assets/tailwind.css">
</head>
<body class="bg-stone-50 text-slate-900">
<div class="min-h-screen bg-stone-50 text-slate-900 md:grid md:grid-cols-[minmax(220px,300px)_minmax(0,1fr)]">
<aside class="border-b border-stone-200 bg-white/70 px-5 py-5 md:min-h-screen md:border-b-0 md:border-r">
<div class="mb-5">
<a class="text-xl font-bold tracking-normal text-slate-950" href="index.html">XP Static Wiki</a>
<p class="mt-1 text-sm text-slate-500">Static HTML export</p>
</div>
<nav class="flex flex-col gap-1">{nav}</nav>
</aside>
<main class="min-w-0">
<header class="flex min-h-16 flex-wrap items-center justify-between gap-3 border-b border-stone-200 bg-white px-5 py-3">
<div class="min-w-0">
<h1 class="truncate text-lg font-semibold text-slate-950">{}</h1>
<p class="text-sm text-slate-500">{}</p>
</div>
</header>
<div class="mx-auto w-full max-w-6xl p-4 md:p-6">
<article class="markdown rounded-lg border border-stone-200 bg-white p-5 shadow-sm md:p-8">{body_html}</article>
</div>
</main>
</div>
</body>
</html>
"#,
        escape_html(&format!("{} - XP Static Wiki", page.title)),
        escape_html(&page.title),
        escape_html(&page.slug),
    )
}

fn export_empty_index_html() -> String {
    r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>XP Static Wiki</title>
<link rel="stylesheet" href="assets/tailwind.css">
</head>
<body class="bg-stone-50 text-slate-900">
<main class="mx-auto w-full max-w-6xl p-4 md:p-6">
<article class="markdown rounded-lg border border-stone-200 bg-white p-5 shadow-sm md:p-8">
<h1>XP Static Wiki</h1>
<p>No pages were available when this export was generated.</p>
</article>
</main>
</body>
</html>
"#
    .to_owned()
}

fn rewrite_export_asset_paths(html: &str) -> String {
    html.replace("src=\"/media/", "src=\"media/")
        .replace("href=\"/media/", "href=\"media/")
        .replace("src=\"/assets/", "src=\"assets/")
        .replace("href=\"/assets/", "href=\"assets/")
}

fn copy_export_media(root: &Path, export_dir: &Path) -> StorageResult<()> {
    let export_media_dir = export_dir.join("media");
    copy_supported_media_files(&root.join(MEDIA_DIR), &export_media_dir, true)?;
    copy_supported_media_files(Path::new(MEDIA_DIR), &export_media_dir, false)?;
    copy_supported_media_files(Path::new("assets"), &export_media_dir, false)?;
    copy_supported_media_files(Path::new("assets"), &export_dir.join("assets"), false)?;
    Ok(())
}

fn copy_supported_media_files(
    source_dir: &Path,
    destination_dir: &Path,
    overwrite_existing: bool,
) -> StorageResult<()> {
    if !source_dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination_dir.join(entry.file_name());
        if source_path.is_dir() {
            copy_supported_media_files(&source_path, &destination_path, overwrite_existing)?;
            continue;
        }
        if media_content_type(&source_path).is_none() {
            continue;
        }
        if destination_path.exists() && !overwrite_existing {
            continue;
        }
        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source_path, destination_path)?;
    }

    Ok(())
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn escape_html_attr(value: &str) -> String {
    escape_html(value)
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

    fn page_summary(slug: &str, title: &str, created_at: Option<i64>) -> PageSummary {
        PageSummary {
            slug: slug.to_owned(),
            title: title.to_owned(),
            categories: Vec::new(),
            promoted: true,
            created_at,
            updated_at: created_at,
            updated_by: None,
        }
    }

    fn categorized_page_summary(slug: &str, title: &str, category: &str) -> PageSummary {
        PageSummary {
            slug: slug.to_owned(),
            title: title.to_owned(),
            categories: vec![category.to_owned()],
            promoted: true,
            created_at: None,
            updated_at: None,
            updated_by: None,
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
    fn save_page_should_reject_empty_title() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");

        let result = store.save_page("guide", "  ", "# Guide", &test_user());

        assert!(matches!(result, Err(StorageError::InvalidTitle(_))));
    }

    #[test]
    fn save_page_should_reject_invalid_slug() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");

        let result = store.save_page("../guide", "Guide", "# Guide", &test_user());

        assert!(matches!(result, Err(StorageError::InvalidSlug(_))));
    }

    #[test]
    fn sort_pages_by_creation_time_should_order_oldest_first() {
        let mut pages = vec![
            page_summary("later", "Later", Some(20)),
            page_summary("unknown", "Unknown", None),
            page_summary("earlier", "Earlier", Some(10)),
        ];

        sort_pages_by_creation_time(&mut pages);

        let slugs = pages
            .iter()
            .map(|page| page.slug.as_str())
            .collect::<Vec<_>>();
        assert_eq!(slugs, vec!["earlier", "later", "unknown"]);
    }

    #[test]
    fn page_categories_from_markdown_should_parse_front_matter_categories() {
        let categories =
            page_categories_from_markdown("---\ncategories: [Test Page, npc]\n---\n\n# Page");

        assert_eq!(categories, vec!["test-page", "npc"]);
    }

    #[test]
    fn page_promoted_from_markdown_should_parse_false_front_matter() {
        let promoted = page_promoted_from_markdown("---\npromoted: false\n---\n\n# Page");

        assert!(!promoted);
    }

    #[test]
    fn expand_category_shortcodes_should_support_bare_category_names() {
        let pages = vec![categorized_page_summary("alpha", "Alpha Page", "test-page")];

        let expanded = expand_category_shortcodes("Pages:\n\n{{test_page}}", &pages);

        assert_eq!(expanded, "Pages:\n\n- [Alpha Page](/alpha)");
    }

    #[test]
    fn read_page_should_expand_category_shortcodes_without_changing_raw_markdown() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");
        store
            .save_page(
                "alpha",
                "Alpha Page",
                "---\ncategories: test-page\n---\n\n# Alpha Page\n\nBody",
                &test_user(),
            )
            .expect("categorized page should save");
        store
            .save_page(
                "index",
                "Index",
                "# Index\n\n{{category:test-page}}",
                &test_user(),
            )
            .expect("index page should save");

        let page = store
            .read_page("index")
            .expect("page should read")
            .expect("page should exist");

        assert_eq!(page.markdown, "# Index\n\n{{category:test-page}}");
        assert_eq!(page.rendered_markdown, "# Index\n\n- [Alpha Page](/alpha)");
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
    fn export_html_site_should_write_pages_assets_and_media() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");

        store
            .save_media_file("", "Hero.PNG", b"image", &test_user())
            .expect("media should be saved");
        store
            .save_page(
                "guide",
                "Guide",
                "# Guide\n\n![Hero](/media/hero.png)",
                &test_user(),
            )
            .expect("page should save");
        let export = store
            .export_html_site()
            .expect("HTML export should be written");

        let export_dir = dir.path().join(EXPORT_DIR).join(EXPORT_LATEST_DIR);
        let html = fs::read_to_string(export_dir.join("guide.html"))
            .expect("exported page should be readable");

        assert_eq!(export.page_count, 1);
        assert_eq!(export.url, "/exports/latest/index.html");
        assert!(export_dir.join("index.html").is_file());
        assert!(export_dir.join("assets").join("tailwind.css").is_file());
        assert_eq!(
            fs::read(export_dir.join("media").join("hero.png"))
                .expect("exported media should be readable"),
            b"image"
        );
        assert!(html.contains(r#"src="media/hero.png""#));
    }

    #[test]
    fn read_export_file_from_root_should_serve_html_files() {
        let dir = tempdir().expect("temp dir should be created");
        let export_root = dir.path().join(EXPORT_DIR);
        fs::create_dir_all(export_root.join(EXPORT_LATEST_DIR))
            .expect("export dir should be created");
        fs::write(
            export_root.join(EXPORT_LATEST_DIR).join("index.html"),
            "<!doctype html>",
        )
        .expect("export file should be written");

        let file = read_export_file_from_root("latest/index.html", &export_root)
            .expect("export file lookup should not fail")
            .expect("export file should exist");

        assert_eq!(file.content_type, "text/html; charset=utf-8");
        assert_eq!(file.contents, b"<!doctype html>");
    }

    #[test]
    fn read_export_file_from_root_should_reject_parent_segments() {
        let dir = tempdir().expect("temp dir should be created");

        let file = read_export_file_from_root("../pages/home.md", dir.path())
            .expect("export file lookup should not fail");

        assert!(file.is_none());
    }

    #[test]
    fn safe_media_rel_path_should_reject_parent_segments() {
        let path = safe_media_rel_path("../roles.json");

        assert!(path.is_none());
    }

    #[test]
    fn safe_media_folder_rel_path_should_allow_root() {
        let path = safe_media_folder_rel_path("");

        assert_eq!(path.as_deref(), Some(Path::new("")));
    }

    #[test]
    fn safe_media_filename_should_sanitize_supported_uploads() {
        let filename = safe_media_filename("Portrait Image.JPG");

        assert_eq!(filename.as_deref(), Some("portrait-image.jpg"));
    }

    #[test]
    fn safe_media_filename_should_reject_nested_uploads() {
        let filename = safe_media_filename("../portrait.png");

        assert!(filename.is_none());
    }

    #[test]
    fn validate_media_file_size_should_reject_files_over_limit() {
        let err = validate_media_file_size(MEDIA_FILE_MAX_BYTES + 1)
            .expect_err("oversized media file should be rejected");

        assert_eq!(
            err.to_string(),
            format!(
                "media file is too large ({} bytes, maximum {} bytes)",
                MEDIA_FILE_MAX_BYTES + 1,
                MEDIA_FILE_MAX_BYTES
            )
        );
    }

    #[test]
    fn list_media_should_include_folders_images_and_videos() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");
        let media_dir = dir.path().join(MEDIA_DIR);
        fs::create_dir_all(media_dir.join("portraits")).expect("folder should be created");
        fs::write(media_dir.join("hero.png"), b"image").expect("image should be written");
        fs::write(media_dir.join("intro.webm"), b"video").expect("video should be written");
        fs::write(media_dir.join("notes.txt"), b"text").expect("text should be written");

        let listing = store.list_media("").expect("media should load");

        assert_eq!(listing.path, "");
        assert_eq!(listing.parent, None);
        assert!(listing.entries.iter().any(|entry| entry.name == "portraits"
            && entry.path == "portraits"
            && entry.kind == MediaEntryKind::Folder));
        assert!(listing.entries.iter().any(|entry| entry.name == "hero.png"
            && entry.path == "hero.png"
            && entry.kind == MediaEntryKind::Image
            && entry.url.as_deref() == Some("/media/hero.png")));
        assert!(listing
            .entries
            .iter()
            .any(|entry| entry.name == "intro.webm"
                && entry.path == "intro.webm"
                && entry.kind == MediaEntryKind::Video
                && entry.url.as_deref() == Some("/media/intro.webm")));
        assert!(!listing
            .entries
            .iter()
            .any(|entry| entry.name == "notes.txt"));
    }

    #[test]
    fn create_media_folder_should_sanitize_name_and_track_placeholder() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");

        let listing = store
            .create_media_folder("", "NPC Portraits", &test_user())
            .expect("folder should be created");

        assert!(dir
            .path()
            .join(MEDIA_DIR)
            .join("npc-portraits")
            .join(".gitkeep")
            .exists());
        assert!(listing
            .entries
            .iter()
            .any(|entry| entry.name == "npc-portraits"
                && entry.path == "npc-portraits"
                && entry.kind == MediaEntryKind::Folder));
    }

    #[test]
    fn save_media_file_should_sanitize_name_and_save_video() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");

        let listing = store
            .save_media_file("", "Intro Clip.MP4", b"video", &test_user())
            .expect("video should be saved");

        assert_eq!(
            fs::read(dir.path().join(MEDIA_DIR).join("intro-clip.mp4"))
                .expect("video should exist"),
            b"video"
        );
        assert!(listing
            .entries
            .iter()
            .any(|entry| entry.name == "intro-clip.mp4"
                && entry.kind == MediaEntryKind::Video
                && entry.url.as_deref() == Some("/media/intro-clip.mp4")));
    }

    #[test]
    fn move_media_file_should_move_image_into_existing_folder() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");
        let media_dir = dir.path().join(MEDIA_DIR);
        fs::create_dir_all(media_dir.join("portraits")).expect("folder should be created");
        fs::write(media_dir.join("hero.png"), b"image").expect("image should be written");

        store
            .move_media_file("hero.png", "portraits", &test_user())
            .expect("image should move");

        assert!(!media_dir.join("hero.png").exists());
        assert_eq!(
            fs::read(media_dir.join("portraits").join("hero.png"))
                .expect("moved image should exist"),
            b"image"
        );
    }

    #[test]
    fn move_media_file_should_reject_existing_destination() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");
        let media_dir = dir.path().join(MEDIA_DIR);
        fs::create_dir_all(media_dir.join("portraits")).expect("folder should be created");
        fs::write(media_dir.join("hero.png"), b"image").expect("image should be written");
        fs::write(media_dir.join("portraits").join("hero.png"), b"existing")
            .expect("existing image should be written");

        let err = store
            .move_media_file("hero.png", "portraits", &test_user())
            .expect_err("move should reject overwrite");

        assert_eq!(
            err.to_string(),
            "media file `portraits/hero.png` already exists"
        );
    }

    #[test]
    fn delete_media_entry_should_remove_file_from_worktree_and_git_tree() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");

        store
            .save_media_file("", "Hero.PNG", b"image", &test_user())
            .expect("image should be saved");
        store
            .delete_media_entry("hero.png", &test_user())
            .expect("image should be deleted");

        assert!(!dir.path().join(MEDIA_DIR).join("hero.png").exists());
        let tree = store
            .head_commit()
            .expect("head commit should exist")
            .tree()
            .expect("tree should load");
        assert!(tree.get_path(Path::new("media/hero.png")).is_err());
    }

    #[test]
    fn delete_media_entry_should_remove_folder_recursively_from_worktree_and_git_tree() {
        let dir = tempdir().expect("temp dir should be created");
        let store = WikiStore::open(dir.path()).expect("store should open");

        store
            .create_media_folder("", "Portraits", &test_user())
            .expect("folder should be created");
        store
            .save_media_file("portraits", "Hero.PNG", b"image", &test_user())
            .expect("image should be saved");
        store
            .delete_media_entry("portraits", &test_user())
            .expect("folder should be deleted");

        assert!(!dir.path().join(MEDIA_DIR).join("portraits").exists());
        let tree = store
            .head_commit()
            .expect("head commit should exist")
            .tree()
            .expect("tree should load");
        assert!(tree.get_path(Path::new("media/portraits")).is_err());
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

    #[test]
    fn read_media_file_from_roots_should_read_video_file() {
        let dir = tempdir().expect("temp dir should be created");
        fs::write(dir.path().join("clip.mp4"), b"video").expect("media file should be written");

        let media = read_media_file_from_roots("clip.mp4", [dir.path().to_path_buf()])
            .expect("media lookup should not fail")
            .expect("media file should be found");

        assert_eq!(media.content_type, "video/mp4");
    }
}
