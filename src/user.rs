use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

pub(crate) const NO_ROLE_LABEL: &str = "none";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AuthUser {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UserAccess {
    pub role: Option<String>,
    pub can_manage_users: bool,
    pub can_manage_settings: bool,
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

#[component]
pub(crate) fn UsersView(
    users_state: Option<ServerFnResult<Vec<ManagedUser>>>,
    can_manage_users: bool,
    managed_user_id: Signal<String>,
    managed_user_name: Signal<String>,
    managed_user_email: Signal<String>,
    managed_user_role: Signal<String>,
    managed_user_locked: Signal<bool>,
    on_new: EventHandler<MouseEvent>,
    on_select: EventHandler<ManagedUser>,
    on_save: EventHandler<MouseEvent>,
    on_delete: EventHandler<String>,
) -> Element {
    let form_is_locked = managed_user_locked();
    let save_disabled = !can_manage_users || form_is_locked || managed_user_id().trim().is_empty();

    rsx! {
        div { class: "grid gap-4 lg:grid-cols-[minmax(0,1fr)_340px]",
            section { class: "rounded-lg border border-stone-200 bg-white p-4 shadow-sm",
                div { class: "mb-3 flex flex-wrap items-center justify-between gap-2",
                    h3 { class: "text-base font-semibold text-slate-950", "Managed Users" }
                    button {
                        class: "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400",
                        onclick: move |event| on_new.call(event),
                        "New User"
                    }
                }
                div { class: "grid gap-2",
                    match users_state {
                        Some(Ok(users)) if users.is_empty() => rsx! {
                            p { class: "py-6 text-sm text-slate-500", "No managed users" }
                        },
                        Some(Ok(users)) => rsx! {
                            for managed_user in users {
                                UserRow {
                                    key: "{managed_user.id}",
                                    managed_user,
                                    active_id: managed_user_id(),
                                    on_select,
                                    on_delete,
                                }
                            }
                        },
                        Some(Err(err)) => rsx! {
                            p { class: "rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-800", "{err}" }
                        },
                        None => rsx! {
                            p { class: "py-6 text-sm text-slate-500", "Loading users" }
                        },
                    }
                }
            }

            section { class: "rounded-lg border border-stone-200 bg-white p-4 shadow-sm",
                h3 { class: "mb-3 text-base font-semibold text-slate-950", "User Role" }
                div { class: "grid gap-3",
                    label { class: "grid gap-1 text-sm font-semibold text-slate-700",
                        "User ID"
                        input {
                            value: "{managed_user_id}",
                            disabled: form_is_locked,
                            oninput: move |event| managed_user_id.set(event.value()),
                            placeholder: "provider-user-id"
                        }
                    }
                    label { class: "grid gap-1 text-sm font-semibold text-slate-700",
                        "Name"
                        input {
                            value: "{managed_user_name}",
                            disabled: form_is_locked,
                            oninput: move |event| managed_user_name.set(event.value()),
                            placeholder: "Display name"
                        }
                    }
                    label { class: "grid gap-1 text-sm font-semibold text-slate-700",
                        "Email"
                        input {
                            value: "{managed_user_email}",
                            disabled: form_is_locked,
                            oninput: move |event| managed_user_email.set(event.value()),
                            placeholder: "user@example.com"
                        }
                    }
                    label { class: "grid gap-1 text-sm font-semibold text-slate-700",
                        "Role"
                        select {
                            value: "{managed_user_role}",
                            disabled: form_is_locked,
                            oninput: move |event| managed_user_role.set(event.value()),
                            option { value: "viewer", "Viewer" }
                            option { value: "editor", "Editor" }
                            option { value: "admin", "Admin" }
                            option { value: NO_ROLE_LABEL, "None" }
                        }
                    }
                    if form_is_locked {
                        p { class: "rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-900", "This user is locked by .env role configuration." }
                    }
                    div { class: "flex flex-wrap gap-2",
                        button {
                            class: "inline-flex h-9 items-center rounded-md border border-emerald-700 bg-emerald-700 px-3 text-sm font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50",
                            disabled: save_disabled,
                            onclick: move |event| on_save.call(event),
                            "Save User"
                        }
                        button {
                            class: "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400",
                            onclick: move |event| on_new.call(event),
                            "Clear"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn UserRow(
    managed_user: ManagedUser,
    active_id: String,
    on_select: EventHandler<ManagedUser>,
    on_delete: EventHandler<String>,
) -> Element {
    let selected = managed_user.id == active_id;
    let class = if selected {
        "grid gap-3 rounded-md border border-emerald-700 bg-emerald-50 p-3 text-left sm:grid-cols-[minmax(0,1fr)_auto]"
    } else {
        "grid gap-3 rounded-md border border-stone-200 bg-white p-3 text-left hover:border-stone-300 sm:grid-cols-[minmax(0,1fr)_auto]"
    };
    let select_user = managed_user.clone();
    let delete_id = managed_user.id.clone();
    let email = managed_user
        .email
        .clone()
        .unwrap_or_else(|| "No email".to_owned());

    rsx! {
        div { class,
            button {
                class: "min-w-0 text-left",
                onclick: move |_| on_select.call(select_user.clone()),
                span { class: "block truncate text-sm font-semibold text-slate-950", "{managed_user.name}" }
                span { class: "block truncate text-xs text-slate-500", "{managed_user.id}" }
                span { class: "block truncate text-xs text-slate-600", "{email}" }
            }
            div { class: "flex items-center gap-2 sm:justify-end",
                span { class: "rounded-md bg-slate-100 px-2 py-1 text-xs font-semibold text-slate-700", "{managed_user.role}" }
                if managed_user.locked {
                    span { class: "rounded-md border border-amber-200 bg-amber-50 px-2 py-1 text-xs font-semibold text-amber-900", "Locked" }
                } else {
                    button {
                        class: "inline-flex h-8 items-center rounded-md border border-red-200 bg-white px-2 text-xs font-semibold text-red-700 hover:border-red-300",
                        onclick: move |_| on_delete.call(delete_id.clone()),
                        "Delete"
                    }
                }
            }
        }
    }
}

#[get("/api/users", headers: dioxus::fullstack::HeaderMap)]
pub(crate) async fn list_managed_users() -> ServerFnResult<Vec<ManagedUser>> {
    let user = authenticated_user_from_headers(&headers)?;
    crate::server::roles::list_managed_users(&user).map_err(role_server_error)
}

#[post("/api/users/save", headers: dioxus::fullstack::HeaderMap)]
pub(crate) async fn save_managed_user(input: ManagedUserInput) -> ServerFnResult<ManagedUser> {
    let user = authenticated_user_from_headers(&headers)?;
    crate::server::roles::save_managed_user(&user, input).map_err(role_server_error)
}

#[post("/api/users/delete", headers: dioxus::fullstack::HeaderMap)]
pub(crate) async fn delete_managed_user(id: String) -> ServerFnResult<()> {
    let user = authenticated_user_from_headers(&headers)?;
    crate::server::roles::delete_managed_user(&user, &id).map_err(role_server_error)
}

#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
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
