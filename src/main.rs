mod markdown;
mod models;
mod slug;

#[cfg(feature = "server")]
mod server;

use dioxus::prelude::*;
use markdown::{compose_page_markdown, render_markdown};
use models::{
    AuthProviderInfo, AuthUser, DiffLineKind, ManagedUser, ManagedUserInput, PageDetail, PageDiff,
    PageRevision, PageSummary, UserAccess,
};
use slug::normalize_slug;

const TAILWIND: Asset = asset!("/assets/tailwind.css");
const NO_ROLE_LABEL: &str = "none";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActiveTab {
    View,
    Edit,
    History,
    Users,
}

fn main() {
    #[cfg(feature = "server")]
    {
        dioxus::serve(|| async move {
            use dioxus::server::axum::routing::get as axum_get;

            let router = dioxus::server::router(App)
                .route(
                    "/auth/login/{provider}",
                    axum_get(server::auth::login_handler),
                )
                .route(
                    "/auth/callback/{provider}",
                    axum_get(server::auth::callback_handler),
                )
                .route("/auth/logout", axum_get(server::auth::logout_handler));

            Ok(router)
        });
    }

    #[cfg(all(not(feature = "server"), feature = "desktop"))]
    {
        let server_url = desktop_server_url();
        dioxus::fullstack::set_server_url(Box::leak(server_url.into_boxed_str()));
        dioxus::launch(App);
    }

    #[cfg(all(not(feature = "server"), feature = "web"))]
    {
        dioxus::launch(App);
    }

    #[cfg(not(any(feature = "server", feature = "web", feature = "desktop")))]
    {
        eprintln!("Enable one of the `server`, `web`, or `desktop` features.");
    }
}

#[component]
fn App() -> Element {
    let mut selected_slug = use_signal(|| "home".to_owned());
    let mut active_tab = use_signal(|| ActiveTab::View);
    let mut editor_title = use_signal(String::new);
    let mut editor_markdown = use_signal(String::new);
    let mut draft_slug = use_signal(|| "home".to_owned());
    let mut selected_revision = use_signal(String::new);
    let mut refresh_key = use_signal(|| 0_u64);
    let mut status = use_signal(String::new);
    let mut managed_user_id = use_signal(String::new);
    let mut managed_user_name = use_signal(String::new);
    let mut managed_user_email = use_signal(String::new);
    let mut managed_user_role = use_signal(|| "viewer".to_owned());
    let mut managed_user_locked = use_signal(|| false);

    let mut user_resource = use_resource(move || async move {
        let _ = refresh_key();
        current_user().await
    });
    let mut user_access_resource = use_resource(move || async move {
        let _ = refresh_key();
        current_user_access().await
    });
    let mut auth_providers_resource = use_resource(move || async move {
        let _ = refresh_key();
        configured_oauth_providers().await
    });
    let mut pages_resource = use_resource(move || async move {
        let _ = refresh_key();
        list_wiki_pages().await
    });
    let mut page_resource = use_resource(move || async move {
        let _ = refresh_key();
        let slug = selected_slug();
        get_wiki_page(slug).await
    });
    let mut history_resource = use_resource(move || async move {
        let _ = refresh_key();
        let slug = selected_slug();
        get_wiki_history(slug).await
    });
    let mut users_resource = use_resource(move || async move {
        let _ = refresh_key();
        if active_tab() == ActiveTab::Users {
            list_managed_users().await
        } else {
            Ok(Vec::new())
        }
    });
    let diff_resource = use_resource(move || async move {
        let slug = selected_slug();
        let revision = selected_revision();
        if revision.is_empty() {
            Ok(None)
        } else {
            get_wiki_diff(slug, revision).await.map(Some)
        }
    });

    let user = user_resource().and_then(Result::ok).flatten();
    let auth_providers_state = auth_providers_resource();
    let pages_state = pages_resource();
    let page_state = page_resource();
    let history_state = history_resource();
    let users_state = users_resource();
    let diff_state = diff_resource();
    let user_access = user_access_resource().and_then(Result::ok);
    let can_manage_users = user_access
        .as_ref()
        .is_some_and(|access| access.can_manage_users);
    let current_user_role = user_access
        .as_ref()
        .and_then(|access| access.role.clone())
        .unwrap_or_else(|| NO_ROLE_LABEL.to_owned());
    let user_is_authenticated = user.is_some();
    let current_page = page_state
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(Clone::clone);
    let selected = selected_slug();
    let active = active_tab();
    let header_title = if active == ActiveTab::Users {
        "Users".to_owned()
    } else {
        current_page
            .as_ref()
            .map(|page| page.title.clone())
            .unwrap_or_else(|| "New page".to_owned())
    };
    let header_context = if active == ActiveTab::Users {
        format!("Role: {current_user_role}")
    } else {
        selected.clone()
    };
    let auth_provider_error = auth_providers_state
        .as_ref()
        .and_then(|providers| providers.as_ref().err())
        .map(ToString::to_string);
    let login_links = auth_providers_state
        .as_ref()
        .and_then(|providers| providers.as_ref().ok())
        .map(|providers| auth_login_links(providers))
        .unwrap_or_default();
    let auth_providers_loading = auth_providers_state.is_none();
    let logout_url = auth_logout_url();

    let open_editor = {
        let current_page = current_page.clone();
        move |_| {
            if let Some(page) = current_page.clone() {
                draft_slug.set(page.slug);
                editor_title.set(page.title);
                editor_markdown.set(page.markdown);
            } else {
                draft_slug.set(selected_slug());
                editor_title.set(String::new());
                editor_markdown.set(String::new());
            }
            active_tab.set(ActiveTab::Edit);
            status.set(String::new());
        }
    };
    let new_managed_user = move |_| {
        managed_user_id.set(String::new());
        managed_user_name.set(String::new());
        managed_user_email.set(String::new());
        managed_user_role.set("viewer".to_owned());
        managed_user_locked.set(false);
        status.set(String::new());
    };
    let edit_managed_user = move |managed_user: ManagedUser| {
        managed_user_id.set(managed_user.id);
        managed_user_name.set(managed_user.name);
        managed_user_email.set(managed_user.email.unwrap_or_default());
        managed_user_role.set(managed_user.role);
        managed_user_locked.set(managed_user.locked);
        status.set(String::new());
    };
    let save_managed_user_action = move |_| async move {
        let email_value = managed_user_email();
        let email = if email_value.trim().is_empty() {
            None
        } else {
            Some(email_value.trim().to_owned())
        };
        let input = ManagedUserInput {
            id: managed_user_id().trim().to_owned(),
            name: managed_user_name().trim().to_owned(),
            email,
            role: managed_user_role(),
        };

        match save_managed_user(input).await {
            Ok(saved) => {
                managed_user_id.set(saved.id);
                managed_user_name.set(saved.name);
                managed_user_email.set(saved.email.unwrap_or_default());
                managed_user_role.set(saved.role);
                managed_user_locked.set(saved.locked);
                refresh_key += 1;
                user_access_resource.restart();
                users_resource.restart();
                status.set("User saved.".to_owned());
            }
            Err(err) => status.set(format!("User save failed: {err}")),
        }
    };
    let delete_managed_user_action = move |id: String| async move {
        match delete_managed_user(id).await {
            Ok(()) => {
                managed_user_id.set(String::new());
                managed_user_name.set(String::new());
                managed_user_email.set(String::new());
                managed_user_role.set("viewer".to_owned());
                managed_user_locked.set(false);
                refresh_key += 1;
                user_access_resource.restart();
                users_resource.restart();
                status.set("User deleted.".to_owned());
            }
            Err(err) => status.set(format!("User delete failed: {err}")),
        }
    };

    rsx! {
        document::Stylesheet { href: TAILWIND }
        div { class: "min-h-screen bg-stone-50 text-slate-900 md:grid md:grid-cols-[minmax(220px,300px)_minmax(0,1fr)]",
            aside { class: "border-b border-stone-200 bg-white/70 px-5 py-5 md:min-h-screen md:border-b-0 md:border-r",
                div { class: "mb-5",
                    h1 { class: "text-xl font-bold tracking-normal text-slate-950", "XP Static Wiki" }
                    p { class: "mt-1 text-sm text-slate-500", "Markdown pages, Git history" }
                }

                div { class: "mb-4 flex flex-wrap gap-2",
                    button {
                        class: "inline-flex h-9 items-center rounded-md border border-emerald-700 bg-emerald-700 px-3 text-sm font-semibold text-white hover:bg-emerald-800",
                        onclick: move |_| {
                            selected_slug.set("new-page".to_owned());
                            draft_slug.set(String::new());
                            editor_title.set(String::new());
                            editor_markdown.set(String::new());
                            selected_revision.set(String::new());
                            active_tab.set(ActiveTab::Edit);
                            status.set(String::new());
                        },
                        "New"
                    }
                    button {
                        class: "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400",
                        onclick: move |_| {
                            refresh_key += 1;
                            user_resource.restart();
                            user_access_resource.restart();
                            auth_providers_resource.restart();
                            pages_resource.restart();
                            page_resource.restart();
                            history_resource.restart();
                            users_resource.restart();
                        },
                        "Refresh"
                    }
                }

                nav { class: "flex flex-col gap-1",
                    match pages_state {
                        Some(Ok(pages)) if pages.is_empty() => rsx! {
                            p { class: "py-6 text-sm text-slate-500", "No pages" }
                        },
                        Some(Ok(pages)) => rsx! {
                            for page in pages {
                                PageNavButton {
                                    page,
                                    selected: selected.clone(),
                                    on_select: move |slug: String| {
                                        selected_slug.set(slug);
                                        active_tab.set(ActiveTab::View);
                                        selected_revision.set(String::new());
                                        status.set(String::new());
                                    }
                                }
                            }
                        },
                        Some(Err(err)) => rsx! {
                            p { class: "rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-800", "{err}" }
                        },
                        None => rsx! {
                            p { class: "py-6 text-sm text-slate-500", "Loading pages" }
                        },
                    }
                }
            }

            main { class: "min-w-0",
                header { class: "flex min-h-16 flex-wrap items-center justify-between gap-3 border-b border-stone-200 bg-white px-5 py-3",
                    div { class: "min-w-0",
                        h2 { class: "truncate text-lg font-semibold text-slate-950",
                            "{header_title}"
                        }
                        p { class: "text-sm text-slate-500", "{header_context}" }
                    }
                    div { class: "flex flex-wrap items-center gap-2",
                        TabButton { label: "View", active: active_tab() == ActiveTab::View, onclick: move |_| active_tab.set(ActiveTab::View) }
                        TabButton { label: "History", active: active_tab() == ActiveTab::History, onclick: move |_| active_tab.set(ActiveTab::History) }
                        if can_manage_users {
                            TabButton {
                                label: "Users",
                                active: active_tab() == ActiveTab::Users,
                                onclick: move |_| {
                                    active_tab.set(ActiveTab::Users);
                                    users_resource.restart();
                                }
                            }
                        }
                        button {
                            class: "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400",
                            onclick: open_editor,
                            "Edit"
                        }
                    }
                }

                div { class: "mx-auto w-full max-w-6xl p-4 md:p-6",
                    div { class: "mb-4 flex flex-wrap items-center justify-between gap-3",
                        AuthStatus {
                            user,
                            login_links,
                            providers_loading: auth_providers_loading,
                            provider_error: auth_provider_error,
                            logout_url
                        }
                        if !status().is_empty() {
                            p { class: "rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-900", "{status}" }
                        }
                    }

                    match active_tab() {
                        ActiveTab::View => rsx! {
                            PageView { page_state }
                        },
                        ActiveTab::Edit => rsx! {
                            PageEditor {
                                user_is_authenticated,
                                draft_slug,
                                editor_title,
                                editor_markdown,
                                on_cancel: move |_| active_tab.set(ActiveTab::View),
                                on_save: move |_| async move {
                                    let Some(slug) = normalize_slug(&draft_slug()) else {
                                        status.set("Choose a valid slug or title.".to_owned());
                                        return;
                                    };
                                    let title = editor_title();
                                    let markdown = compose_page_markdown(&title, &editor_markdown());
                                    match save_wiki_page(slug.clone(), title, markdown).await {
                                        Ok(saved) => {
                                            selected_slug.set(saved.slug.clone());
                                            draft_slug.set(saved.slug);
                                            editor_title.set(saved.title);
                                            editor_markdown.set(saved.markdown);
                                            selected_revision.set(String::new());
                                            active_tab.set(ActiveTab::View);
                                            refresh_key += 1;
                                            user_resource.restart();
                                            pages_resource.restart();
                                            page_resource.restart();
                                            history_resource.restart();
                                            status.set("Saved.".to_owned());
                                        }
                                        Err(err) => status.set(format!("Save failed: {err}")),
                                    }
                                }
                            }
                        },
                        ActiveTab::History => rsx! {
                            HistoryView {
                                history_state,
                                diff_state,
                                selected_revision,
                            }
                        },
                        ActiveTab::Users => rsx! {
                            UsersView {
                                users_state,
                                can_manage_users,
                                managed_user_id,
                                managed_user_name,
                                managed_user_email,
                                managed_user_role,
                                managed_user_locked,
                                on_new: new_managed_user,
                                on_select: edit_managed_user,
                                on_save: save_managed_user_action,
                                on_delete: delete_managed_user_action,
                            }
                        },
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct LoginLink {
    label: String,
    url: String,
}

#[component]
fn AuthStatus(
    user: Option<AuthUser>,
    login_links: Vec<LoginLink>,
    providers_loading: bool,
    provider_error: Option<String>,
    logout_url: String,
) -> Element {
    let provider_error = provider_error.unwrap_or_default();
    let has_provider_error = !provider_error.is_empty();

    rsx! {
        div { class: "flex flex-wrap items-center gap-2 text-sm",
            match user {
                Some(user) => rsx! {
                    span { class: "rounded-md bg-slate-100 px-3 py-2 text-slate-700", "{user.name}" }
                    a {
                        class: "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 font-semibold text-slate-800 hover:border-stone-400",
                        href: "{logout_url}",
                        "Sign out"
                    }
                },
                None if providers_loading => rsx! {
                    span { class: "rounded-md bg-slate-100 px-3 py-2 text-slate-600", "Loading sign-in" }
                },
                None if has_provider_error => rsx! {
                    span { class: "rounded-md border border-red-200 bg-red-50 px-3 py-2 text-red-800", "{provider_error}" }
                },
                None if login_links.is_empty() => rsx! {
                    span { class: "rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-amber-900", "Configure an OAuth provider in .env" }
                },
                None => rsx! {
                    for link in login_links {
                        a {
                            key: "{link.label}",
                            class: "inline-flex h-9 items-center rounded-md border border-emerald-700 bg-emerald-700 px-3 font-semibold text-white hover:bg-emerald-800",
                            href: "{link.url}",
                            "Sign in with {link.label}"
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn TabButton(label: &'static str, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let class = if active {
        "inline-flex h-9 items-center rounded-md border border-emerald-700 bg-emerald-700 px-3 text-sm font-semibold text-white"
    } else {
        "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400"
    };

    rsx! {
        button { class, onclick: move |event| onclick.call(event), "{label}" }
    }
}

#[component]
fn PageNavButton(page: PageSummary, selected: String, on_select: EventHandler<String>) -> Element {
    let class = if page.slug == selected {
        "flex min-h-10 w-full items-center justify-between gap-3 rounded-md border border-stone-300 bg-white px-3 text-left text-sm font-semibold text-slate-950"
    } else {
        "flex min-h-10 w-full items-center justify-between gap-3 rounded-md border border-transparent bg-transparent px-3 text-left text-sm font-medium text-slate-700 hover:bg-white"
    };
    let slug = page.slug.clone();

    rsx! {
        button {
            key: "{page.slug}",
            class,
            onclick: move |_| on_select.call(slug.clone()),
            span { class: "truncate", "{page.title}" }
        }
    }
}

#[component]
fn PageView(page_state: Option<ServerFnResult<Option<PageDetail>>>) -> Element {
    rsx! {
        match page_state {
            Some(Ok(Some(page))) => {
                let html = render_markdown(&page.markdown);
                rsx! {
                    article {
                        class: "markdown rounded-lg border border-stone-200 bg-white p-5 shadow-sm md:p-8",
                        dangerous_inner_html: "{html}"
                    }
                }
            },
            Some(Ok(None)) => rsx! {
                div { class: "rounded-lg border border-dashed border-stone-300 bg-white p-8 text-slate-500", "Page not found" }
            },
            Some(Err(err)) => rsx! {
                div { class: "rounded-lg border border-red-200 bg-red-50 p-5 text-red-800", "{err}" }
            },
            None => rsx! {
                div { class: "rounded-lg border border-stone-200 bg-white p-8 text-slate-500", "Loading page" }
            },
        }
    }
}

#[component]
fn PageEditor(
    user_is_authenticated: bool,
    draft_slug: Signal<String>,
    editor_title: Signal<String>,
    editor_markdown: Signal<String>,
    on_cancel: EventHandler<MouseEvent>,
    on_save: EventHandler<MouseEvent>,
) -> Element {
    let preview_html = render_markdown(&compose_page_markdown(&editor_title(), &editor_markdown()));

    rsx! {
        div { class: "grid gap-4 lg:grid-cols-2",
            section { class: "rounded-lg border border-stone-200 bg-white p-5 shadow-sm",
                div { class: "mb-4 grid gap-3",
                    label { class: "grid gap-1 text-sm font-semibold text-slate-700",
                        "Slug"
                        input {
                            value: "{draft_slug}",
                            oninput: move |event| draft_slug.set(event.value()),
                            placeholder: "page-slug"
                        }
                    }
                    label { class: "grid gap-1 text-sm font-semibold text-slate-700",
                        "Title"
                        input {
                            value: "{editor_title}",
                            oninput: move |event| editor_title.set(event.value()),
                            placeholder: "Page title"
                        }
                    }
                    label { class: "grid gap-1 text-sm font-semibold text-slate-700",
                        "Markdown"
                        textarea {
                            value: "{editor_markdown}",
                            oninput: move |event| editor_markdown.set(event.value()),
                            placeholder: "# Page title"
                        }
                    }
                }
                div { class: "flex flex-wrap items-center gap-2",
                    button {
                        class: "inline-flex h-9 items-center rounded-md border border-emerald-700 bg-emerald-700 px-3 text-sm font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50",
                        disabled: !user_is_authenticated,
                        onclick: move |event| on_save.call(event),
                        "Save"
                    }
                    button {
                        class: "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400",
                        onclick: move |event| on_cancel.call(event),
                        "Cancel"
                    }
                    if !user_is_authenticated {
                        span { class: "text-sm text-slate-500", "Sign in to save changes" }
                    }
                }
            }

            section { class: "rounded-lg border border-stone-200 bg-white p-5 shadow-sm",
                article {
                    class: "markdown",
                    dangerous_inner_html: "{preview_html}"
                }
            }
        }
    }
}

#[component]
fn HistoryView(
    history_state: Option<ServerFnResult<Vec<PageRevision>>>,
    diff_state: Option<ServerFnResult<Option<PageDiff>>>,
    selected_revision: Signal<String>,
) -> Element {
    rsx! {
        div { class: "grid gap-4 lg:grid-cols-[320px_minmax(0,1fr)]",
            section { class: "rounded-lg border border-stone-200 bg-white p-4 shadow-sm",
                h3 { class: "mb-3 text-base font-semibold text-slate-950", "Revisions" }
                div { class: "flex flex-col gap-2",
                    match history_state {
                        Some(Ok(revisions)) if revisions.is_empty() => rsx! {
                            p { class: "text-sm text-slate-500", "No revisions" }
                        },
                        Some(Ok(revisions)) => rsx! {
                            for revision in revisions {
                                RevisionButton {
                                    revision,
                                    active_revision: selected_revision(),
                                    on_select: move |id: String| selected_revision.set(id),
                                }
                            }
                        },
                        Some(Err(err)) => rsx! {
                            p { class: "rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-800", "{err}" }
                        },
                        None => rsx! {
                            p { class: "text-sm text-slate-500", "Loading history" }
                        },
                    }
                }
            }

            section { class: "min-w-0 rounded-lg border border-stone-200 bg-white p-4 shadow-sm",
                h3 { class: "mb-3 text-base font-semibold text-slate-950", "Diff" }
                match diff_state {
                    Some(Ok(Some(diff))) => rsx! {
                        DiffView { diff }
                    },
                    Some(Ok(None)) => rsx! {
                        p { class: "text-sm text-slate-500", "Select a revision" }
                    },
                    Some(Err(err)) => rsx! {
                        p { class: "rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-800", "{err}" }
                    },
                    None => rsx! {
                        p { class: "text-sm text-slate-500", "Loading diff" }
                    },
                }
            }
        }
    }
}

#[component]
fn UsersView(
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

#[component]
fn RevisionButton(
    revision: PageRevision,
    active_revision: String,
    on_select: EventHandler<String>,
) -> Element {
    let class = if revision.id == active_revision {
        "w-full rounded-md border border-emerald-700 bg-emerald-50 p-3 text-left text-sm"
    } else {
        "w-full rounded-md border border-stone-200 bg-white p-3 text-left text-sm hover:border-stone-300"
    };
    let id = revision.id.clone();

    rsx! {
        button {
            key: "{revision.id}",
            class,
            onclick: move |_| on_select.call(id.clone()),
            span { class: "block font-mono font-semibold text-slate-950", "{revision.short_id}" }
            span { class: "block truncate text-slate-700", "{revision.summary}" }
            span { class: "block text-xs text-slate-500", "{revision.author} · {revision.timestamp}" }
        }
    }
}

#[component]
fn DiffView(diff: PageDiff) -> Element {
    rsx! {
        div { class: "overflow-x-auto rounded-lg border border-stone-200",
            for line in diff.lines {
                DiffLineView { line }
            }
        }
    }
}

#[component]
fn DiffLineView(line: models::DiffLine) -> Element {
    let class = match line.kind {
        DiffLineKind::Addition => "diff-line bg-emerald-50",
        DiffLineKind::Deletion => "diff-line bg-red-50",
        DiffLineKind::Hunk => "diff-line bg-indigo-50 font-semibold text-indigo-900",
        DiffLineKind::Context => "diff-line bg-white",
    };
    let old_lineno = line
        .old_lineno
        .map(|line| line.to_string())
        .unwrap_or_default();
    let new_lineno = line
        .new_lineno
        .map(|line| line.to_string())
        .unwrap_or_default();

    rsx! {
        div { class,
            span { class: "text-right text-slate-500", "{old_lineno}" }
            span { class: "text-right text-slate-500", "{new_lineno}" }
            span { class: "min-w-0", "{line.content}" }
        }
    }
}

#[get("/api/session", headers: dioxus::fullstack::HeaderMap)]
async fn current_user() -> ServerFnResult<Option<AuthUser>> {
    Ok(server::auth::current_user_from_headers(&headers))
}

#[get("/api/session/access", headers: dioxus::fullstack::HeaderMap)]
async fn current_user_access() -> ServerFnResult<UserAccess> {
    match server::auth::current_user_from_headers(&headers) {
        Some(user) => server::roles::access_for_user(&user).map_err(role_server_error),
        None => Ok(UserAccess {
            role: None,
            can_manage_users: false,
        }),
    }
}

#[get("/api/auth/providers")]
async fn configured_oauth_providers() -> ServerFnResult<Vec<AuthProviderInfo>> {
    Ok(server::auth::configured_providers())
}

#[get("/api/pages")]
async fn list_wiki_pages() -> ServerFnResult<Vec<PageSummary>> {
    server::storage::list_pages().map_err(server_error)
}

#[get("/api/pages/{slug}")]
async fn get_wiki_page(slug: String) -> ServerFnResult<Option<PageDetail>> {
    server::storage::read_page(&slug).map_err(server_error)
}

#[post("/api/pages/save", headers: dioxus::fullstack::HeaderMap)]
async fn save_wiki_page(
    slug: String,
    title: String,
    markdown: String,
) -> ServerFnResult<PageDetail> {
    let user = server::auth::current_user_from_headers(&headers).ok_or_else(|| {
        ServerFnError::ServerError {
            message: "sign in to save changes".to_owned(),
            code: 401,
            details: None,
        }
    })?;

    let normalized = normalize_slug(&slug).ok_or_else(|| ServerFnError::ServerError {
        message: "invalid page slug".to_owned(),
        code: 400,
        details: None,
    })?;
    server::roles::ensure_can_write_page(&user, &normalized).map_err(role_server_error)?;

    server::storage::save_page(&normalized, &title, &markdown, &user).map_err(server_error)
}

#[get("/api/pages/{slug}/history")]
async fn get_wiki_history(slug: String) -> ServerFnResult<Vec<PageRevision>> {
    server::storage::page_history(&slug).map_err(server_error)
}

#[get("/api/pages/{slug}/diff/{revision}")]
async fn get_wiki_diff(slug: String, revision: String) -> ServerFnResult<PageDiff> {
    server::storage::page_diff(&slug, &revision).map_err(server_error)
}

#[get("/api/users", headers: dioxus::fullstack::HeaderMap)]
async fn list_managed_users() -> ServerFnResult<Vec<ManagedUser>> {
    let user = authenticated_user_from_headers(&headers)?;
    server::roles::list_managed_users(&user).map_err(role_server_error)
}

#[post("/api/users/save", headers: dioxus::fullstack::HeaderMap)]
async fn save_managed_user(input: ManagedUserInput) -> ServerFnResult<ManagedUser> {
    let user = authenticated_user_from_headers(&headers)?;
    server::roles::save_managed_user(&user, input).map_err(role_server_error)
}

#[post("/api/users/delete", headers: dioxus::fullstack::HeaderMap)]
async fn delete_managed_user(id: String) -> ServerFnResult<()> {
    let user = authenticated_user_from_headers(&headers)?;
    server::roles::delete_managed_user(&user, &id).map_err(role_server_error)
}

#[cfg(feature = "server")]
fn authenticated_user_from_headers(
    headers: &dioxus::fullstack::HeaderMap,
) -> ServerFnResult<AuthUser> {
    server::auth::current_user_from_headers(headers).ok_or_else(|| ServerFnError::ServerError {
        message: "sign in required".to_owned(),
        code: 401,
        details: None,
    })
}

#[cfg(feature = "server")]
fn server_error(err: impl ToString) -> ServerFnError {
    ServerFnError::ServerError {
        message: err.to_string(),
        code: 500,
        details: None,
    }
}

#[cfg(feature = "server")]
fn role_server_error(err: server::roles::RoleAccessError) -> ServerFnError {
    let code = match err {
        server::roles::RoleAccessError::Forbidden { .. } => 403,
        server::roles::RoleAccessError::InvalidUser => 400,
        server::roles::RoleAccessError::LockedUser(_)
        | server::roles::RoleAccessError::LastAdmin => 409,
        server::roles::RoleAccessError::UnknownRole(_)
        | server::roles::RoleAccessError::RoleSystem(_)
        | server::roles::RoleAccessError::Lock
        | server::roles::RoleAccessError::UserStore(_) => 500,
    };
    ServerFnError::ServerError {
        message: err.to_string(),
        code,
        details: None,
    }
}

fn auth_login_links(providers: &[AuthProviderInfo]) -> Vec<LoginLink> {
    providers
        .iter()
        .map(|provider| LoginLink {
            label: provider.label.clone(),
            url: auth_login_url(&provider.slug),
        })
        .collect()
}

fn auth_login_url(provider: &str) -> String {
    #[cfg(feature = "desktop")]
    {
        return format!(
            "{}/auth/login/{provider}",
            desktop_server_url().trim_end_matches('/')
        );
    }

    #[cfg(not(feature = "desktop"))]
    {
        format!("/auth/login/{provider}")
    }
}

fn auth_logout_url() -> String {
    #[cfg(feature = "desktop")]
    {
        return format!("{}/auth/logout", desktop_server_url().trim_end_matches('/'));
    }

    #[cfg(not(feature = "desktop"))]
    {
        "/auth/logout".to_owned()
    }
}

#[cfg(feature = "desktop")]
fn desktop_server_url() -> String {
    std::env::var("XP_WIKI_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned())
}
