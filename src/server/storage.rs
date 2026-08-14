use crate::markdown::{humanize_slug, title_from_markdown};
use crate::models::{
    AuthUser, DiffLine, DiffLineKind, PageDetail, PageDiff, PageRevision, PageSummary,
};
use crate::slug::is_valid_slug;
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
const DEFAULT_HOME: &str = "# Home\n\nWelcome to your Rust and Dioxus wiki.\n";

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

pub fn page_history(slug: &str) -> StorageResult<Vec<PageRevision>> {
    with_store(|store| store.page_history(slug))
}

pub fn page_diff(slug: &str, revision: &str) -> StorageResult<PageDiff> {
    with_store(|store| store.page_diff(slug, revision))
}

fn with_store<T>(operation: impl FnOnce(&WikiStore) -> StorageResult<T>) -> StorageResult<T> {
    let _guard = GIT_LOCK
        .lock()
        .map_err(|_| git2::Error::from_str("the wiki git lock was poisoned"))?;
    let store = WikiStore::open(data_dir())?;
    store.ensure_seed_page()?;
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
}
