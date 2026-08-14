use crate::server::env::{load_dotenv, optional_env};
use crate::user::{AuthUser, ManagedUser, ManagedUserInput, UserAccess};
use once_cell::sync::Lazy;
use role_system::{
    storage::FileStorage, Permission, Resource, Role, RoleSystem, RoleSystemConfig, Subject,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;

static ROLE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
const PAGE_RESOURCE_TYPE: &str = "wiki_pages";
const USER_RESOURCE_TYPE: &str = "wiki_users";
const READ_ACTION: &str = "read";
const WRITE_ACTION: &str = "write";
const MANAGE_ACTION: &str = "manage";
const NO_ROLE: &str = "none";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WikiRole {
    Admin,
    Editor,
    Viewer,
}

#[derive(Debug, Error)]
pub enum RoleAccessError {
    #[error("role-system operation failed: {0}")]
    RoleSystem(#[from] role_system::Error),
    #[error("role-system lock was poisoned")]
    Lock,
    #[error("unknown wiki role `{0}` in role configuration")]
    UnknownRole(String),
    #[error("user store operation failed: {0}")]
    UserStore(String),
    #[error("managed user `{0}` is locked by .env role configuration")]
    LockedUser(String),
    #[error("managed user id is required")]
    InvalidUser,
    #[error("at least one admin user must remain")]
    LastAdmin,
    #[error("user `{user}` is not allowed to {action} `{resource}`")]
    Forbidden {
        user: String,
        action: &'static str,
        resource: String,
    },
}

type RoleResult<T> = Result<T, RoleAccessError>;

#[derive(Debug)]
struct RolePolicy {
    role_file_path: PathBuf,
    user_file_path: PathBuf,
    default_role: Option<WikiRole>,
    admin_users: Vec<String>,
    editor_users: Vec<String>,
    viewer_users: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ManagedUserRecord {
    id: String,
    name: String,
    email: Option<String>,
    role: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct UserStore {
    users: BTreeMap<String, ManagedUserRecord>,
}

pub fn ensure_can_write_page(user: &AuthUser, slug: &str) -> RoleResult<()> {
    let policy = RolePolicy::from_env()?;
    ensure_can_write_page_with_policy(user, slug, &policy)
}

pub fn record_authenticated_user(user: &AuthUser) -> RoleResult<()> {
    let policy = RolePolicy::from_env()?;
    record_authenticated_user_with_policy(user, &policy)
}

pub fn access_for_user(user: &AuthUser) -> RoleResult<UserAccess> {
    let policy = RolePolicy::from_env()?;
    access_for_user_with_policy(user, &policy)
}

pub fn list_managed_users(user: &AuthUser) -> RoleResult<Vec<ManagedUser>> {
    let policy = RolePolicy::from_env()?;
    list_managed_users_with_policy(user, &policy)
}

pub fn save_managed_user(user: &AuthUser, input: ManagedUserInput) -> RoleResult<ManagedUser> {
    let policy = RolePolicy::from_env()?;
    save_managed_user_with_policy(user, input, &policy)
}

pub fn delete_managed_user(user: &AuthUser, id: &str) -> RoleResult<()> {
    let policy = RolePolicy::from_env()?;
    delete_managed_user_with_policy(user, id, &policy)
}

fn ensure_can_write_page_with_policy(
    user: &AuthUser,
    slug: &str,
    policy: &RolePolicy,
) -> RoleResult<()> {
    let _guard = ROLE_LOCK.lock().map_err(|_| RoleAccessError::Lock)?;
    let mut system = initialized_system(policy)?;
    let store = UserStore::load(&policy.user_file_path)?;
    let subject = subject_for_user(user);
    assign_user_role(&mut system, &subject, user, policy, &store)?;

    let resource = Resource::new_checked(slug, PAGE_RESOURCE_TYPE)?;
    if system.check_permission(&subject, WRITE_ACTION, &resource)? {
        Ok(())
    } else {
        Err(RoleAccessError::Forbidden {
            user: user.name.clone(),
            action: WRITE_ACTION,
            resource: slug.to_owned(),
        })
    }
}

fn record_authenticated_user_with_policy(user: &AuthUser, policy: &RolePolicy) -> RoleResult<()> {
    let _guard = ROLE_LOCK.lock().map_err(|_| RoleAccessError::Lock)?;
    let mut store = UserStore::load(&policy.user_file_path)?;
    let role = match configured_role_for_user(user, policy) {
        Some(role) => Some(role),
        None => match store.users.get(&user.id) {
            Some(record) => role_from_optional_name(record.role.as_deref())?,
            None => policy.default_role,
        },
    };

    store.users.insert(
        user.id.clone(),
        ManagedUserRecord {
            id: user.id.clone(),
            name: user.name.clone(),
            email: user.email.clone(),
            role: role.map(|role| role.name().to_owned()),
        },
    );
    store.save(&policy.user_file_path)
}

fn access_for_user_with_policy(user: &AuthUser, policy: &RolePolicy) -> RoleResult<UserAccess> {
    let _guard = ROLE_LOCK.lock().map_err(|_| RoleAccessError::Lock)?;
    let mut system = initialized_system(policy)?;
    let store = UserStore::load(&policy.user_file_path)?;
    let subject = subject_for_user(user);
    assign_user_role(&mut system, &subject, user, policy, &store)?;

    Ok(UserAccess {
        role: effective_role_for_user(user, policy, &store)?.map(|role| role.name().to_owned()),
        can_manage_users: can_manage_users(&system, &subject)?,
    })
}

fn list_managed_users_with_policy(
    current_user: &AuthUser,
    policy: &RolePolicy,
) -> RoleResult<Vec<ManagedUser>> {
    let _guard = ROLE_LOCK.lock().map_err(|_| RoleAccessError::Lock)?;
    let mut system = initialized_system(policy)?;
    let store = UserStore::load(&policy.user_file_path)?;
    let subject = subject_for_user(current_user);
    assign_user_role(&mut system, &subject, current_user, policy, &store)?;
    ensure_can_manage_users(&system, &subject, current_user)?;

    Ok(managed_users_from_store(policy, &store))
}

fn save_managed_user_with_policy(
    current_user: &AuthUser,
    input: ManagedUserInput,
    policy: &RolePolicy,
) -> RoleResult<ManagedUser> {
    let _guard = ROLE_LOCK.lock().map_err(|_| RoleAccessError::Lock)?;
    let mut system = initialized_system(policy)?;
    let mut store = UserStore::load(&policy.user_file_path)?;
    let subject = subject_for_user(current_user);
    assign_user_role(&mut system, &subject, current_user, policy, &store)?;
    ensure_can_manage_users(&system, &subject, current_user)?;

    let record = record_from_input(input)?;
    if user_record_is_locked(&record, policy) {
        return Err(RoleAccessError::LockedUser(record.id));
    }

    if removing_last_admin(&record.id, record.role.as_deref(), policy, &store)? {
        return Err(RoleAccessError::LastAdmin);
    }

    store.users.insert(record.id.clone(), record.clone());
    store.save(&policy.user_file_path)?;
    Ok(managed_user_from_record(policy, &record))
}

fn delete_managed_user_with_policy(
    current_user: &AuthUser,
    id: &str,
    policy: &RolePolicy,
) -> RoleResult<()> {
    let _guard = ROLE_LOCK.lock().map_err(|_| RoleAccessError::Lock)?;
    let mut system = initialized_system(policy)?;
    let mut store = UserStore::load(&policy.user_file_path)?;
    let subject = subject_for_user(current_user);
    assign_user_role(&mut system, &subject, current_user, policy, &store)?;
    ensure_can_manage_users(&system, &subject, current_user)?;

    if let Some(record) = store.users.get(id) {
        if user_record_is_locked(record, policy) {
            return Err(RoleAccessError::LockedUser(id.to_owned()));
        }
    }

    if removing_last_admin(id, Some(NO_ROLE), policy, &store)? {
        return Err(RoleAccessError::LastAdmin);
    }

    store.users.remove(id);
    store.save(&policy.user_file_path)
}

fn initialized_system(policy: &RolePolicy) -> RoleResult<RoleSystem<FileStorage>> {
    let storage = FileStorage::new(&policy.role_file_path)?;
    let mut system = RoleSystem::with_storage(storage, RoleSystemConfig::default());
    ensure_wiki_roles(&mut system)?;
    Ok(system)
}

fn ensure_wiki_roles(system: &mut RoleSystem<FileStorage>) -> RoleResult<()> {
    ensure_role(system, viewer_role()?)?;
    ensure_role(system, editor_role()?)?;
    ensure_role(system, admin_role()?)?;
    system.add_role_inheritance(WikiRole::Editor.name(), WikiRole::Viewer.name())?;
    system.add_role_inheritance(WikiRole::Admin.name(), WikiRole::Editor.name())?;
    Ok(())
}

fn ensure_role(system: &mut RoleSystem<FileStorage>, role: Role) -> RoleResult<()> {
    let role_name = role.name().to_owned();
    if system.get_role(&role_name)?.is_some() {
        system.update_role(role)?;
    } else {
        system.register_role(role)?;
    }
    Ok(())
}

fn viewer_role() -> RoleResult<Role> {
    Ok(Role::new(WikiRole::Viewer.name())
        .with_description("Read wiki pages")
        .add_permission(page_permission(READ_ACTION)?))
}

fn editor_role() -> RoleResult<Role> {
    Ok(Role::new(WikiRole::Editor.name())
        .with_description("Read and edit wiki pages")
        .add_permission(page_permission(READ_ACTION)?)
        .add_permission(page_permission(WRITE_ACTION)?))
}

fn admin_role() -> RoleResult<Role> {
    Ok(Role::new(WikiRole::Admin.name())
        .with_description("Full wiki administration")
        .add_permission(page_permission(READ_ACTION)?)
        .add_permission(page_permission(WRITE_ACTION)?)
        .add_permission(user_permission(MANAGE_ACTION)?)
        .add_permission(Permission::try_new("*", "*")?))
}

fn page_permission(action: &'static str) -> RoleResult<Permission> {
    Ok(Permission::try_new(action, PAGE_RESOURCE_TYPE)?)
}

fn user_permission(action: &'static str) -> RoleResult<Permission> {
    Ok(Permission::try_new(action, USER_RESOURCE_TYPE)?)
}

fn ensure_can_manage_users(
    system: &RoleSystem<FileStorage>,
    subject: &Subject,
    user: &AuthUser,
) -> RoleResult<()> {
    if can_manage_users(system, subject)? {
        Ok(())
    } else {
        Err(RoleAccessError::Forbidden {
            user: user.name.clone(),
            action: MANAGE_ACTION,
            resource: "users".to_owned(),
        })
    }
}

fn can_manage_users(system: &RoleSystem<FileStorage>, subject: &Subject) -> RoleResult<bool> {
    let resource = Resource::new_checked("users", USER_RESOURCE_TYPE)?;
    Ok(system.check_permission(subject, MANAGE_ACTION, &resource)?)
}

fn assign_user_role(
    system: &mut RoleSystem<FileStorage>,
    subject: &Subject,
    user: &AuthUser,
    policy: &RolePolicy,
    store: &UserStore,
) -> RoleResult<()> {
    if let Some(role) = effective_role_for_user(user, policy, store)? {
        system.assign_role(subject, role.name())?;
    }

    Ok(())
}

fn effective_role_for_user(
    user: &AuthUser,
    policy: &RolePolicy,
    store: &UserStore,
) -> RoleResult<Option<WikiRole>> {
    if let Some(role) = configured_role_for_user(user, policy) {
        return Ok(Some(role));
    }

    if let Some(record) = store.users.get(&user.id) {
        return role_from_optional_name(record.role.as_deref());
    }

    Ok(policy.default_role)
}

fn configured_role_for_user(user: &AuthUser, policy: &RolePolicy) -> Option<WikiRole> {
    if user_is_listed(user, &policy.admin_users) {
        return Some(WikiRole::Admin);
    }
    if user_is_listed(user, &policy.editor_users) {
        return Some(WikiRole::Editor);
    }
    if user_is_listed(user, &policy.viewer_users) {
        return Some(WikiRole::Viewer);
    }
    None
}

fn role_from_optional_name(role: Option<&str>) -> RoleResult<Option<WikiRole>> {
    match role.map(str::trim).filter(|role| !role.is_empty()) {
        Some(NO_ROLE) | None => Ok(None),
        Some(role) => WikiRole::from_name(role).map(Some),
    }
}

fn record_from_input(input: ManagedUserInput) -> RoleResult<ManagedUserRecord> {
    let id = input.id.trim();
    if id.is_empty() {
        return Err(RoleAccessError::InvalidUser);
    }

    let name = input.name.trim();
    let email = input
        .email
        .map(|email| email.trim().to_owned())
        .filter(|email| !email.is_empty());
    let role = role_from_optional_name(Some(input.role.trim()))?.map(|role| role.name().to_owned());

    Ok(ManagedUserRecord {
        id: id.to_owned(),
        name: if name.is_empty() {
            id.to_owned()
        } else {
            name.to_owned()
        },
        email,
        role,
    })
}

fn subject_for_user(user: &AuthUser) -> Subject {
    Subject::user(&user.id).with_display_name(&user.name)
}

fn user_is_listed(user: &AuthUser, values: &[String]) -> bool {
    values.iter().any(|value| {
        value == &user.id
            || value == &user.name
            || user
                .email
                .as_ref()
                .is_some_and(|email| value.eq_ignore_ascii_case(email))
    })
}

fn user_record_is_locked(record: &ManagedUserRecord, policy: &RolePolicy) -> bool {
    configured_role_for_record(record, policy).is_some()
}

fn configured_role_for_record(record: &ManagedUserRecord, policy: &RolePolicy) -> Option<WikiRole> {
    if record_is_listed(record, &policy.admin_users) {
        return Some(WikiRole::Admin);
    }
    if record_is_listed(record, &policy.editor_users) {
        return Some(WikiRole::Editor);
    }
    if record_is_listed(record, &policy.viewer_users) {
        return Some(WikiRole::Viewer);
    }
    None
}

fn effective_role_for_record(
    record: &ManagedUserRecord,
    policy: &RolePolicy,
) -> RoleResult<Option<WikiRole>> {
    if let Some(role) = configured_role_for_record(record, policy) {
        return Ok(Some(role));
    }
    role_from_optional_name(record.role.as_deref())
}

fn record_is_listed(record: &ManagedUserRecord, values: &[String]) -> bool {
    values.iter().any(|value| {
        value == &record.id
            || value == &record.name
            || record
                .email
                .as_ref()
                .is_some_and(|email| value.eq_ignore_ascii_case(email))
    })
}

fn managed_users_from_store(policy: &RolePolicy, store: &UserStore) -> Vec<ManagedUser> {
    let mut users: BTreeMap<String, ManagedUser> = store
        .users
        .values()
        .map(|record| (record.id.clone(), managed_user_from_record(policy, record)))
        .collect();

    add_locked_env_users(&mut users, WikiRole::Admin, &policy.admin_users);
    add_locked_env_users(&mut users, WikiRole::Editor, &policy.editor_users);
    add_locked_env_users(&mut users, WikiRole::Viewer, &policy.viewer_users);

    users.into_values().collect()
}

fn add_locked_env_users(
    users: &mut BTreeMap<String, ManagedUser>,
    role: WikiRole,
    configured_users: &[String],
) {
    for user in configured_users {
        if user.is_empty()
            || users
                .values()
                .any(|existing| managed_user_matches(existing, user))
        {
            continue;
        }
        users.insert(
            user.clone(),
            ManagedUser {
                id: user.clone(),
                name: user.clone(),
                email: user.contains('@').then(|| user.clone()),
                role: role.name().to_owned(),
                locked: true,
            },
        );
    }
}

fn managed_user_matches(user: &ManagedUser, value: &str) -> bool {
    user.id == value
        || user.name == value
        || user
            .email
            .as_ref()
            .is_some_and(|email| value.eq_ignore_ascii_case(email))
}

fn managed_user_from_record(policy: &RolePolicy, record: &ManagedUserRecord) -> ManagedUser {
    let configured_role = configured_role_for_record(record, policy);
    let role = configured_role
        .or_else(|| {
            role_from_optional_name(record.role.as_deref())
                .ok()
                .flatten()
        })
        .map(|role| role.name().to_owned())
        .unwrap_or_else(|| NO_ROLE.to_owned());

    ManagedUser {
        id: record.id.clone(),
        name: record.name.clone(),
        email: record.email.clone(),
        role,
        locked: configured_role.is_some(),
    }
}

fn removing_last_admin(
    target_id: &str,
    replacement_role: Option<&str>,
    policy: &RolePolicy,
    store: &UserStore,
) -> RoleResult<bool> {
    let target_was_admin = store
        .users
        .get(target_id)
        .map(|record| effective_role_for_record(record, policy))
        .transpose()?
        .flatten()
        == Some(WikiRole::Admin);
    let replacement_is_admin = role_from_optional_name(replacement_role)? == Some(WikiRole::Admin);

    if !target_was_admin || replacement_is_admin {
        return Ok(false);
    }

    Ok(!store.users.values().any(|record| {
        record.id != target_id
            && effective_role_for_record(record, policy).ok().flatten() == Some(WikiRole::Admin)
    }) && policy.admin_users.is_empty())
}

impl UserStore {
    fn load(path: &Path) -> RoleResult<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let json = fs::read_to_string(path).map_err(|err| {
            RoleAccessError::UserStore(format!("failed to read {}: {err}", path.display()))
        })?;
        serde_json::from_str(&json).map_err(|err| {
            RoleAccessError::UserStore(format!("failed to parse {}: {err}", path.display()))
        })
    }

    fn save(&self, path: &Path) -> RoleResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                RoleAccessError::UserStore(format!(
                    "failed to create user store directory {}: {err}",
                    parent.display()
                ))
            })?;
        }

        let json = serde_json::to_string_pretty(self).map_err(|err| {
            RoleAccessError::UserStore(format!("failed to serialize user store: {err}"))
        })?;
        fs::write(path, json).map_err(|err| {
            RoleAccessError::UserStore(format!("failed to write {}: {err}", path.display()))
        })
    }
}

impl RolePolicy {
    fn from_env() -> RoleResult<Self> {
        load_env();
        Ok(Self {
            role_file_path: role_file_path(),
            user_file_path: user_file_path(),
            default_role: match optional_env("XP_WIKI_DEFAULT_ROLE").as_deref() {
                Some("none") => None,
                Some(role) => Some(WikiRole::from_name(role)?),
                None => Some(WikiRole::Editor),
            },
            admin_users: env_list("XP_WIKI_ADMIN_USERS"),
            editor_users: env_list("XP_WIKI_EDITOR_USERS"),
            viewer_users: env_list("XP_WIKI_VIEWER_USERS"),
        })
    }
}

impl WikiRole {
    fn from_name(name: &str) -> RoleResult<Self> {
        match name {
            "admin" => Ok(Self::Admin),
            "editor" => Ok(Self::Editor),
            "viewer" => Ok(Self::Viewer),
            other => Err(RoleAccessError::UnknownRole(other.to_owned())),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }
}

fn role_file_path() -> PathBuf {
    optional_env("XP_WIKI_ROLE_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir().join("roles.json"))
}

fn user_file_path() -> PathBuf {
    optional_env("XP_WIKI_USER_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir().join("users.json"))
}

fn data_dir() -> PathBuf {
    optional_env("XP_WIKI_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("wiki-data"))
}

fn load_env() {
    load_dotenv();
}

fn env_list(key: &str) -> Vec<String> {
    optional_env(key)
        .map(|value| {
            value
                .split([',', ' ', '\n', '\t'])
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_user() -> AuthUser {
        AuthUser {
            id: "user-1".to_owned(),
            name: "Test User".to_owned(),
            email: Some("test@example.com".to_owned()),
        }
    }

    #[test]
    fn ensure_can_write_page_should_persist_roles_to_filesystem_backend() {
        let dir = tempdir().expect("temp dir should be created");
        let policy = RolePolicy {
            role_file_path: dir.path().join("roles.json"),
            user_file_path: dir.path().join("users.json"),
            default_role: Some(WikiRole::Editor),
            admin_users: Vec::new(),
            editor_users: Vec::new(),
            viewer_users: Vec::new(),
        };

        ensure_can_write_page_with_policy(&test_user(), "home", &policy)
            .expect("default editor should write");

        assert!(policy.role_file_path.exists());
    }

    #[test]
    fn ensure_can_write_page_should_deny_viewer() {
        let dir = tempdir().expect("temp dir should be created");
        let policy = RolePolicy {
            role_file_path: dir.path().join("roles.json"),
            user_file_path: dir.path().join("users.json"),
            default_role: None,
            admin_users: Vec::new(),
            editor_users: Vec::new(),
            viewer_users: vec!["test@example.com".to_owned()],
        };

        let result = ensure_can_write_page_with_policy(&test_user(), "home", &policy);

        assert!(matches!(result, Err(RoleAccessError::Forbidden { .. })));
    }

    #[test]
    fn save_managed_user_should_persist_role_when_admin() {
        let dir = tempdir().expect("temp dir should be created");
        let admin = AuthUser {
            id: "admin-1".to_owned(),
            name: "Admin User".to_owned(),
            email: Some("admin@example.com".to_owned()),
        };
        let policy = RolePolicy {
            role_file_path: dir.path().join("roles.json"),
            user_file_path: dir.path().join("users.json"),
            default_role: None,
            admin_users: vec!["admin@example.com".to_owned()],
            editor_users: Vec::new(),
            viewer_users: Vec::new(),
        };

        let saved = save_managed_user_with_policy(
            &admin,
            ManagedUserInput {
                id: "editor-1".to_owned(),
                name: "Editor User".to_owned(),
                email: Some("editor@example.com".to_owned()),
                role: "editor".to_owned(),
            },
            &policy,
        )
        .expect("admin should save managed user");

        assert_eq!(saved.role, "editor");
        assert!(policy.user_file_path.exists());
    }

    #[test]
    fn save_managed_user_should_deny_viewer() {
        let dir = tempdir().expect("temp dir should be created");
        let policy = RolePolicy {
            role_file_path: dir.path().join("roles.json"),
            user_file_path: dir.path().join("users.json"),
            default_role: None,
            admin_users: Vec::new(),
            editor_users: Vec::new(),
            viewer_users: vec!["test@example.com".to_owned()],
        };

        let result = save_managed_user_with_policy(
            &test_user(),
            ManagedUserInput {
                id: "editor-1".to_owned(),
                name: "Editor User".to_owned(),
                email: Some("editor@example.com".to_owned()),
                role: "editor".to_owned(),
            },
            &policy,
        );

        assert!(matches!(result, Err(RoleAccessError::Forbidden { .. })));
    }

    #[test]
    fn managed_none_role_should_override_default_role() {
        let dir = tempdir().expect("temp dir should be created");
        let admin = AuthUser {
            id: "admin-1".to_owned(),
            name: "Admin User".to_owned(),
            email: Some("admin@example.com".to_owned()),
        };
        let managed = AuthUser {
            id: "managed-1".to_owned(),
            name: "Managed User".to_owned(),
            email: Some("managed@example.com".to_owned()),
        };
        let policy = RolePolicy {
            role_file_path: dir.path().join("roles.json"),
            user_file_path: dir.path().join("users.json"),
            default_role: Some(WikiRole::Editor),
            admin_users: vec!["admin@example.com".to_owned()],
            editor_users: Vec::new(),
            viewer_users: Vec::new(),
        };
        save_managed_user_with_policy(
            &admin,
            ManagedUserInput {
                id: managed.id.clone(),
                name: managed.name.clone(),
                email: managed.email.clone(),
                role: NO_ROLE.to_owned(),
            },
            &policy,
        )
        .expect("admin should save managed user");

        let result = ensure_can_write_page_with_policy(&managed, "home", &policy);

        assert!(matches!(result, Err(RoleAccessError::Forbidden { .. })));
    }
}
