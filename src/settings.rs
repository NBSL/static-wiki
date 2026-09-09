use crate::models::{AuthProviderInfo, SettingsOverview};
#[cfg(any(feature = "server", feature = "local"))]
use crate::user::{AuthUser, NO_ROLE_LABEL};
use dioxus::prelude::*;
#[cfg(any(feature = "server", feature = "local"))]
use std::path::{Path, PathBuf};

#[component]
pub(crate) fn SettingsView(
    settings_state: Option<ServerFnResult<SettingsOverview>>,
    can_manage_settings: bool,
    mut refresh_key: Signal<u64>,
) -> Element {
    if !can_manage_settings {
        return rsx! {
            section { class: "rounded-lg border border-amber-200 bg-amber-50 p-5 text-amber-950",
                h3 { class: "text-base font-semibold", "Admin access required" }
                p { class: "mt-1 text-sm", "Settings are locked to admin users." }
            }
        };
    }

    rsx! {
        div { class: "grid gap-4",
            div { class: "flex flex-wrap items-center justify-between gap-3",
                div { class: "min-w-0",
                    h3 { class: "text-base font-semibold text-slate-950", "Application Settings" }
                    p { class: "mt-1 text-sm text-slate-500", "Read-only runtime configuration for this wiki." }
                }
                button {
                    class: "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400",
                    onclick: move |_| {
                        refresh_key.set(refresh_key() + 1);
                    },
                    "Refresh"
                }
            }

            match settings_state {
                Some(Ok(settings)) => {
                    let auth_providers = auth_provider_summary(&settings.configured_auth_providers);
                    rsx! {
                        div { class: "grid gap-4 lg:grid-cols-2",
                            section { class: "rounded-lg border border-stone-200 bg-white p-4 shadow-sm",
                                h4 { class: "mb-3 text-sm font-semibold uppercase tracking-normal text-slate-500", "Access" }
                                dl { class: "grid gap-2",
                                    SettingsRow { label: "Current role", value: settings.current_role.clone() }
                                    SettingsRow { label: "Settings access", value: "Admin only".to_owned() }
                                    SettingsRow { label: "OAuth providers", value: auth_providers }
                                    SettingsRow { label: "Default role", value: settings.default_role.clone() }
                                }
                            }

                            section { class: "rounded-lg border border-stone-200 bg-white p-4 shadow-sm",
                                h4 { class: "mb-3 text-sm font-semibold uppercase tracking-normal text-slate-500", "Storage" }
                                dl { class: "grid gap-2",
                                    SettingsRow { label: "Data directory", value: settings.data_dir.clone() }
                                    SettingsRow { label: "Pages", value: settings.pages_dir.clone() }
                                    SettingsRow { label: "Templates", value: settings.templates_dir.clone() }
                                    SettingsRow { label: "Components", value: settings.components_dir.clone() }
                                    SettingsRow { label: "Media", value: settings.media_dir.clone() }
                                    SettingsRow { label: "HTML export", value: settings.export_dir.clone() }
                                }
                            }

                            section { class: "rounded-lg border border-stone-200 bg-white p-4 shadow-sm",
                                h4 { class: "mb-3 text-sm font-semibold uppercase tracking-normal text-slate-500", "Role Files" }
                                dl { class: "grid gap-2",
                                    SettingsRow { label: "Roles", value: settings.role_file.clone() }
                                    SettingsRow { label: "Users", value: settings.user_file.clone() }
                                    SettingsRow { label: "Sessions", value: settings.session_file.clone() }
                                }
                            }

                            section { class: "rounded-lg border border-stone-200 bg-white p-4 shadow-sm",
                                h4 { class: "mb-3 text-sm font-semibold uppercase tracking-normal text-slate-500", "Limits" }
                                dl { class: "grid gap-2",
                                    SettingsRow { label: "Media upload", value: settings.media_upload_limit.clone() }
                                }
                            }
                        }
                    }
                },
                Some(Err(err)) => rsx! {
                    p { class: "rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-800", "{err}" }
                },
                None => rsx! {
                    p { class: "rounded-lg border border-stone-200 bg-white p-5 text-sm text-slate-500 shadow-sm", "Loading settings" }
                },
            }
        }
    }
}

#[component]
fn SettingsRow(label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "grid gap-1 border-b border-stone-100 pb-2 last:border-b-0 sm:grid-cols-[140px_minmax(0,1fr)]",
            dt { class: "text-sm font-semibold text-slate-700", "{label}" }
            dd { class: "min-w-0 break-words font-mono text-sm text-slate-900", "{value}" }
        }
    }
}

fn auth_provider_summary(providers: &[AuthProviderInfo]) -> String {
    if providers.is_empty() {
        "None configured".to_owned()
    } else {
        providers
            .iter()
            .map(|provider| provider.label.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg_attr(not(feature = "local"), get("/api/settings", headers: dioxus::fullstack::HeaderMap))]
pub(crate) async fn load_settings_overview() -> ServerFnResult<SettingsOverview> {
    #[cfg(feature = "local")]
    let headers = dioxus::fullstack::HeaderMap::new();
    let user = authenticated_user_from_headers(&headers)?;
    crate::server::roles::ensure_can_manage_settings(&user).map_err(role_server_error)?;
    settings_overview_for_user(&user)
}

#[cfg(any(feature = "server", feature = "local"))]
fn settings_overview_for_user(user: &AuthUser) -> ServerFnResult<SettingsOverview> {
    let access = crate::server::roles::access_for_user(user).map_err(role_server_error)?;
    let data_dir = configured_data_dir();

    Ok(SettingsOverview {
        current_role: access.role.unwrap_or_else(|| NO_ROLE_LABEL.to_owned()),
        data_dir: display_path(&data_dir),
        pages_dir: display_path(data_dir.join("pages")),
        templates_dir: display_path(data_dir.join("templates")),
        components_dir: display_path(data_dir.join("components")),
        media_dir: display_path(data_dir.join("media")),
        export_dir: display_path(data_dir.join("export").join("latest")),
        role_file: display_path(configured_path(
            "XP_WIKI_ROLE_FILE",
            data_dir.join("roles.json"),
        )),
        user_file: display_path(configured_path(
            "XP_WIKI_USER_FILE",
            data_dir.join("users.json"),
        )),
        session_file: display_path(configured_path(
            "XP_WIKI_SESSION_FILE",
            data_dir.join("sessions.json"),
        )),
        default_role: crate::server::env::optional_env("XP_WIKI_DEFAULT_ROLE")
            .unwrap_or_else(|| "editor".to_owned()),
        media_upload_limit: format_byte_limit(crate::server::storage::MEDIA_FILE_MAX_BYTES),
        configured_auth_providers: crate::server::auth::configured_providers(),
    })
}

#[cfg(any(feature = "server", feature = "local"))]
fn configured_data_dir() -> PathBuf {
    crate::server::env::optional_env("XP_WIKI_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("wiki-data"))
}

#[cfg(any(feature = "server", feature = "local"))]
fn configured_path(key: &str, fallback: PathBuf) -> PathBuf {
    crate::server::env::optional_env(key)
        .map(PathBuf::from)
        .unwrap_or(fallback)
}

#[cfg(any(feature = "server", feature = "local"))]
fn display_path(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string()
}

#[cfg(any(feature = "server", feature = "local"))]
fn format_byte_limit(bytes: usize) -> String {
    const MIB: usize = 1024 * 1024;
    if bytes.is_multiple_of(MIB) {
        format!("{} MiB", bytes / MIB)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(any(feature = "server", feature = "local"))]
fn authenticated_user_from_headers(
    headers: &dioxus::fullstack::HeaderMap,
) -> ServerFnResult<AuthUser> {
    crate::server::auth::current_user_from_headers(headers).ok_or_else(|| {
        ServerFnError::ServerError {
            message: "sign in required".to_owned(),
            code: 401,
            details: None,
        }
    })
}

#[cfg(any(feature = "server", feature = "local"))]
fn role_server_error(err: crate::server::roles::RoleAccessError) -> ServerFnError {
    let code = match err {
        crate::server::roles::RoleAccessError::Forbidden { .. } => 403,
        crate::server::roles::RoleAccessError::InvalidUser => 400,
        crate::server::roles::RoleAccessError::LockedUser(_)
        | crate::server::roles::RoleAccessError::LastAdmin => 409,
        crate::server::roles::RoleAccessError::UnknownRole(_)
        | crate::server::roles::RoleAccessError::RoleSystem(_)
        | crate::server::roles::RoleAccessError::Lock
        | crate::server::roles::RoleAccessError::UserStore(_) => 500,
    };
    ServerFnError::ServerError {
        message: err.to_string(),
        code,
        details: None,
    }
}
