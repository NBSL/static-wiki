mod builder;
mod markdown;
mod markdown_components;
mod media;
mod models;
mod settings;
mod slug;
mod user;

#[cfg(feature = "server")]
mod server;

use builder::{EditorMode, ModeButton, PageBuilder};
use dioxus::prelude::*;
use markdown::{
    categories_text_from_page_markdown, compose_page_markdown_with_metadata,
    editable_body_from_page_markdown, promoted_from_page_markdown,
    render_markdown_with_component_manifests,
};
use media::{list_media_entries, MediaManager};
use models::{
    AuthProviderInfo, DiffLineKind, HtmlExport, PageDetail, PageDiff, PageRevision, PageSummary,
    PageTemplateDraft, PageTemplateSummary,
};
use settings::{load_settings_overview, SettingsView};
use slug::{normalize_page_slug, normalize_slug, validate_page_title};
use user::{
    delete_managed_user, list_managed_users, save_managed_user, AuthUser, ManagedUser,
    ManagedUserInput, UserAccess, UsersView, NO_ROLE_LABEL,
};

const TAILWIND: Asset = asset!("/assets/tailwind.css");
#[cfg(feature = "server")]
const SERVER_FUNCTION_BODY_LIMIT_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActiveTab {
    View,
    Edit,
    History,
    Media,
    Users,
    Settings,
}

#[derive(Clone, Debug, PartialEq, Routable)]
enum Route {
    #[redirect("/", || Route::Page {
        slug: "home".to_owned(),
    })]
    #[route("/:slug", WikiPage)]
    Page { slug: String },
}

fn markdown_component_manifests(state: &Option<ServerFnResult<Vec<String>>>) -> Vec<String> {
    state
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .unwrap_or_default()
}

fn main() {
    #[cfg(feature = "server")]
    {
        dioxus::serve(|| async move {
            use dioxus::server::{
                axum::{extract::DefaultBodyLimit, routing::get as axum_get, Router},
                DioxusRouterExt, ServeConfig,
            };

            let router = Router::new()
                .route("/media/{*path}", axum_get(media_handler))
                .route("/exports/{*path}", axum_get(export_handler))
                .route(
                    "/auth/login/{provider}",
                    axum_get(server::auth::login_handler),
                )
                .route(
                    "/auth/callback/{provider}",
                    axum_get(server::auth::callback_handler),
                )
                .route("/auth/logout", axum_get(server::auth::logout_handler))
                .serve_dioxus_application(ServeConfig::new(), App)
                .layer(DefaultBodyLimit::max(SERVER_FUNCTION_BODY_LIMIT_BYTES));

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
    rsx! { Router::<Route> {} }
}

#[component]
fn WikiPage(slug: String) -> Element {
    rsx! { WikiApp { route_slug: slug } }
}

#[component]
fn WikiApp(route_slug: String) -> Element {
    let mut selected_slug = use_signal(|| route_slug.clone());
    let mut active_tab = use_signal(|| ActiveTab::View);
    let mut editor_title = use_signal(String::new);
    let mut editor_categories = use_signal(String::new);
    let mut editor_promoted = use_signal(|| true);
    let mut editor_markdown = use_signal(String::new);
    let mut draft_slug = use_signal(|| "home".to_owned());
    let selected_template_slug = use_signal(String::new);
    let editor_is_new_page = use_signal(|| false);
    let mut selected_revision = use_signal(String::new);
    let mut refresh_key = use_signal(|| 0_u64);
    let mut status = use_signal(String::new);
    let managed_user_id = use_signal(String::new);
    let managed_user_name = use_signal(String::new);
    let managed_user_email = use_signal(String::new);
    let managed_user_role = use_signal(|| "viewer".to_owned());
    let managed_user_locked = use_signal(|| false);
    let mut export_html_url = use_signal(String::new);
    let media_path = use_signal(String::new);
    let new_media_folder_name = use_signal(String::new);
    let editor_signals = EditorSignals {
        draft_slug,
        editor_title,
        editor_categories,
        editor_promoted,
        editor_markdown,
        selected_template_slug,
        editor_is_new_page,
    };
    let managed_user_signals = ManagedUserSignals {
        id: managed_user_id,
        name: managed_user_name,
        email: managed_user_email,
        role: managed_user_role,
        locked: managed_user_locked,
    };

    use_effect(use_reactive!(|route_slug| {
        if selected_slug() != route_slug {
            selected_slug.set(route_slug.clone());
            active_tab.set(ActiveTab::View);
            selected_revision.set(String::new());
            status.set(String::new());
        }
    }));

    let navigator = use_navigator();

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
    let mut templates_resource = use_resource(move || async move {
        let _ = refresh_key();
        list_page_templates().await
    });
    let mut markdown_components_resource = use_resource(move || async move {
        let _ = refresh_key();
        list_markdown_component_manifests().await
    });
    let mut media_resource = use_resource(move || async move {
        let _ = refresh_key();
        let path = media_path();
        list_media_entries(path).await
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
    let mut settings_resource = use_resource(move || async move {
        let _ = refresh_key();
        if active_tab() == ActiveTab::Settings {
            Some(load_settings_overview().await)
        } else {
            None
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
    let templates_state = templates_resource();
    let markdown_components_state = markdown_components_resource();
    let media_state = media_resource();
    let page_state = page_resource();
    let history_state = history_resource();
    let users_state = users_resource();
    let settings_state = settings_resource().flatten();
    let diff_state = diff_resource();
    let user_access = user_access_resource().and_then(Result::ok);
    let can_manage_users = user_access
        .as_ref()
        .is_some_and(|access| access.can_manage_users);
    let can_manage_settings = user_access
        .as_ref()
        .is_some_and(|access| access.can_manage_settings);
    let current_user_role = user_access
        .as_ref()
        .and_then(|access| access.role.as_deref())
        .unwrap_or(NO_ROLE_LABEL);
    let can_manage_content = matches!(current_user_role, "admin" | "editor");
    let can_manage_media = can_manage_content;
    let can_export_html = can_manage_content;
    let user_is_authenticated = user.is_some();
    let current_page = page_state
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(Clone::clone);
    let selected = selected_slug();
    let active = active_tab();
    let header_title = match active {
        ActiveTab::Users => "Users".to_owned(),
        ActiveTab::Media => "Media".to_owned(),
        ActiveTab::Settings => "Settings".to_owned(),
        _ => current_page
            .as_ref()
            .map(|page| page.title.clone())
            .unwrap_or_else(|| "New page".to_owned()),
    };
    let header_context = match active {
        ActiveTab::Users | ActiveTab::Settings => format!("Role: {current_user_role}"),
        ActiveTab::Media => {
            let path = media_path();
            if path.is_empty() {
                "Media root".to_owned()
            } else {
                format!("Media / {path}")
            }
        }
        _ => selected,
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
        move |_| {
            if let Some(page) = current_page.as_ref() {
                editor_signals.populate(page);
            } else {
                editor_signals.reset(selected_slug());
            }
            active_tab.set(ActiveTab::Edit);
            status.set(String::new());
        }
    };
    let apply_template_action = move |_| async move {
        let template_slug = selected_template_slug();
        if template_slug.trim().is_empty() {
            status.set("Choose a template.".to_owned());
            return;
        }

        let normalized_slug = normalize_slug(&draft_slug())
            .or_else(|| normalize_slug(&editor_title()))
            .unwrap_or_else(|| template_slug.clone());

        match apply_page_template(template_slug, normalized_slug.clone(), editor_title()).await {
            Ok(draft) => {
                let categories = categories_text_from_page_markdown(&draft.markdown);
                let promoted = promoted_from_page_markdown(&draft.markdown);
                let markdown = editable_body_from_page_markdown(&draft.markdown);
                draft_slug.set(normalized_slug);
                editor_title.set(draft.title);
                if !categories.is_empty() {
                    editor_categories.set(categories);
                }
                editor_promoted.set(promoted);
                editor_markdown.set(markdown);
                status.set("Template applied.".to_owned());
            }
            Err(err) => status.set(format!("Template failed: {err}")),
        }
    };
    let new_managed_user = move |_| {
        managed_user_signals.reset();
        status.set(String::new());
    };
    let edit_managed_user = move |managed_user: ManagedUser| {
        managed_user_signals.populate(managed_user);
        status.set(String::new());
    };
    let save_managed_user_action = move |_| async move {
        let input = managed_user_signals.input();

        match save_managed_user(input).await {
            Ok(saved) => {
                managed_user_signals.populate(saved);
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
                managed_user_signals.reset();
                refresh_key += 1;
                user_access_resource.restart();
                users_resource.restart();
                status.set("User deleted.".to_owned());
            }
            Err(err) => status.set(format!("User delete failed: {err}")),
        }
    };
    let export_html_action = move |_| async move {
        if !can_export_html {
            status.set("Sign in as an editor or admin to export HTML.".to_owned());
            export_html_url.set(String::new());
            return;
        }

        status.set("Exporting HTML...".to_owned());
        export_html_url.set(String::new());
        match export_wiki_html().await {
            Ok(export) => {
                export_html_url.set(app_server_url(&export.url));
                status.set(format!(
                    "Exported {} pages to {}.",
                    export.page_count, export.path
                ));
            }
            Err(err) => status.set(format!("Export failed: {err}")),
        }
    };

    rsx! {
        document::Stylesheet { href: TAILWIND }
        // document::Stylesheet { href: "https://cdn.jsdelivr.net/npm/tacit-css@1.9.7/dist/tacit-css.min.css"}
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
                            editor_signals.reset(String::new());
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
                            templates_resource.restart();
                            markdown_components_resource.restart();
                            media_resource.restart();
                            page_resource.restart();
                            history_resource.restart();
                            users_resource.restart();
                            settings_resource.restart();
                        },
                        "Refresh"
                    }
                }

                nav { class: "flex flex-col gap-1",
                    match pages_state {
                        Some(Ok(pages)) if pages.is_empty() => rsx! {
                            p { class: "py-6 text-sm text-slate-500", "No pages" }
                        },
                        Some(Ok(pages)) => {
                            let promoted_pages = pages
                                .into_iter()
                                .filter(|page| page.promoted)
                                .collect::<Vec<_>>();
                            rsx! {
                                if promoted_pages.is_empty() {
                                    p { class: "py-6 text-sm text-slate-500", "No promoted pages" }
                                } else {
                                    for page in promoted_pages {
                                        PageNavButton {
                                            page,
                                            selected: selected_slug,
                                            on_open: move |_| {
                                                active_tab.set(ActiveTab::View);
                                                selected_revision.set(String::new());
                                                status.set(String::new());
                                            },
                                        }
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
                        TabButton {
                            label: "Media",
                            active: active_tab() == ActiveTab::Media,
                            onclick: move |_| {
                                active_tab.set(ActiveTab::Media);
                                media_resource.restart();
                            }
                        }
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
                        if can_manage_settings {
                            TabButton {
                                label: "Settings",
                                active: active_tab() == ActiveTab::Settings,
                                onclick: move |_| {
                                    active_tab.set(ActiveTab::Settings);
                                    settings_resource.restart();
                                }
                            }
                        }
                        if can_export_html {
                            button {
                                class: "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400",
                                onclick: export_html_action,
                                "Export HTML"
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
                            div { class: "flex flex-wrap items-center gap-2 rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-900",
                                span { "{status}" }
                                if !export_html_url().is_empty() && status().starts_with("Exported ") {
                                    a {
                                        class: "font-semibold underline",
                                        href: "{export_html_url}",
                                        "Open export"
                                    }
                                }
                            }
                        }
                    }

                    match active_tab() {
                        ActiveTab::View => rsx! {
                            PageView {
                                page_state,
                                component_manifests: markdown_component_manifests(&markdown_components_state)
                            }
                        },
                        ActiveTab::Edit => rsx! {
                            PageEditor {
                                user_is_authenticated,
                                draft_slug,
                                templates_enabled: editor_is_new_page(),
                                templates_state,
                                selected_template_slug,
                                component_manifests: markdown_component_manifests(&markdown_components_state),
                                editor_title,
                                editor_categories,
                                editor_promoted,
                                editor_markdown,
                                on_apply_template: apply_template_action,
                                on_cancel: move |_| active_tab.set(ActiveTab::View),
                                on_save: move |_| async move {
                                    let page_input = match prepare_page_save(
                                        &editor_title(),
                                        &draft_slug(),
                                        &editor_categories(),
                                        editor_promoted(),
                                        &editor_markdown(),
                                    ) {
                                        Ok(input) => input,
                                        Err(error) => {
                                            status.set(error.to_string());
                                            return;
                                        }
                                    };

                                    match save_wiki_page(
                                        page_input.slug,
                                        page_input.title,
                                        page_input.markdown,
                                    )
                                    .await
                                    {
                                        Ok(saved) => {
                                            let saved_slug = saved.slug.clone();
                                            selected_slug.set(saved_slug.clone());
                                            editor_signals.populate(&saved);
                                            let _ = navigator.push(Route::Page { slug: saved_slug });
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
                                },
                            }
                        },
                        ActiveTab::History => rsx! {
                            HistoryView {
                                history_state,
                                diff_state,
                                selected_revision,
                            }
                        },
                        ActiveTab::Media => rsx! {
                            MediaManager {
                                media_state,
                                media_path,
                                new_folder_name: new_media_folder_name,
                                can_manage_media,
                                refresh_key,
                                status,
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
                        ActiveTab::Settings => rsx! {
                            SettingsView {
                                settings_state,
                                can_manage_settings,
                                refresh_key,
                            }
                        },
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PageSaveInput {
    slug: String,
    title: String,
    markdown: String,
}

fn prepare_page_save(
    title: &str,
    draft_slug: &str,
    categories: &str,
    promoted: bool,
    body: &str,
) -> Result<PageSaveInput, slug::PageValidationError> {
    validate_page_title(title)?;
    let slug_source = if draft_slug.trim().is_empty() {
        title
    } else {
        draft_slug
    };
    let slug = normalize_page_slug(slug_source)?;
    let markdown = compose_page_markdown_with_metadata(title, categories, promoted, body);

    Ok(PageSaveInput {
        slug,
        title: title.to_owned(),
        markdown,
    })
}

#[derive(Clone, Copy)]
struct EditorSignals {
    draft_slug: Signal<String>,
    editor_title: Signal<String>,
    editor_categories: Signal<String>,
    editor_promoted: Signal<bool>,
    editor_markdown: Signal<String>,
    selected_template_slug: Signal<String>,
    editor_is_new_page: Signal<bool>,
}

impl EditorSignals {
    fn reset(self, draft: String) {
        let mut signals = self;
        signals.draft_slug.set(draft);
        signals.editor_title.set(String::new());
        signals.editor_categories.set(String::new());
        signals.editor_promoted.set(true);
        signals.editor_markdown.set(String::new());
        signals.selected_template_slug.set(String::new());
        signals.editor_is_new_page.set(true);
    }

    fn populate(self, page: &PageDetail) {
        let mut signals = self;
        signals.draft_slug.set(page.slug.clone());
        signals.editor_title.set(page.title.clone());
        signals.editor_categories.set(page.categories.join(", "));
        signals.editor_promoted.set(page.promoted);
        signals
            .editor_markdown
            .set(editable_body_from_page_markdown(&page.markdown));
        signals.editor_is_new_page.set(false);
    }
}

#[derive(Clone, Copy)]
struct ManagedUserSignals {
    id: Signal<String>,
    name: Signal<String>,
    email: Signal<String>,
    role: Signal<String>,
    locked: Signal<bool>,
}

impl ManagedUserSignals {
    fn reset(self) {
        let mut signals = self;
        signals.id.set(String::new());
        signals.name.set(String::new());
        signals.email.set(String::new());
        signals.role.set("viewer".to_owned());
        signals.locked.set(false);
    }

    fn populate(self, user: ManagedUser) {
        let mut signals = self;
        signals.id.set(user.id);
        signals.name.set(user.name);
        signals.email.set(user.email.unwrap_or_default());
        signals.role.set(user.role);
        signals.locked.set(user.locked);
    }

    fn input(self) -> ManagedUserInput {
        let id = (self.id)();
        let name = (self.name)();
        let email = (self.email)();

        ManagedUserInput {
            id: id.trim().to_owned(),
            name: name.trim().to_owned(),
            email: (!email.trim().is_empty()).then_some(email.trim().to_owned()),
            role: (self.role)(),
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
fn PageNavButton(
    page: PageSummary,
    selected: Signal<String>,
    on_open: EventHandler<()>,
) -> Element {
    let PageSummary { slug, title, .. } = page;
    let class = if slug == selected() {
        "flex min-h-10 w-full items-center justify-between gap-3 rounded-md border border-stone-300 bg-white px-3 text-left text-sm font-semibold text-slate-950"
    } else {
        "flex min-h-10 w-full items-center justify-between gap-3 rounded-md border border-transparent bg-transparent px-3 text-left text-sm font-medium text-slate-700 hover:bg-white"
    };

    rsx! {
        Link {
            key: "{slug}",
            to: Route::Page { slug },
            class,
            onclick: move |_| on_open.call(()),
            span { class: "truncate", "{title}" }
        }
    }
}

#[component]
fn PageView(
    page_state: Option<ServerFnResult<Option<PageDetail>>>,
    component_manifests: Vec<String>,
) -> Element {
    rsx! {
        match page_state {
            Some(Ok(Some(page))) => {
                let html = render_markdown_with_component_manifests(&page.rendered_markdown, &component_manifests);
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
    templates_enabled: bool,
    templates_state: Option<ServerFnResult<Vec<PageTemplateSummary>>>,
    selected_template_slug: Signal<String>,
    component_manifests: Vec<String>,
    editor_title: Signal<String>,
    editor_categories: Signal<String>,
    editor_promoted: Signal<bool>,
    editor_markdown: Signal<String>,
    on_apply_template: EventHandler<MouseEvent>,
    on_cancel: EventHandler<MouseEvent>,
    on_save: EventHandler<MouseEvent>,
) -> Element {
    let mut editor_mode = use_signal(|| EditorMode::Builder);
    let preview_html = render_markdown_with_component_manifests(
        &compose_page_markdown_with_metadata(
            &editor_title(),
            &editor_categories(),
            editor_promoted(),
            &editor_markdown(),
        ),
        &component_manifests,
    );
    let selected_template = selected_template_slug();
    let template_select_disabled =
        !matches!(&templates_state, Some(Ok(templates)) if !templates.is_empty());
    let apply_template_disabled = selected_template.trim().is_empty();

    rsx! {
        div { class: "grid gap-4 lg:grid-cols-2",
            section { class: "rounded-lg border border-stone-200 bg-white p-5 shadow-sm",
                div { class: "mb-4 grid gap-3",
                    if templates_enabled {
                        label { class: "grid gap-1 text-sm font-semibold text-slate-700",
                            "Template"
                            div { class: "flex flex-wrap gap-2",
                                select {
                                    class: "min-w-48 flex-1",
                                    value: "{selected_template}",
                                    disabled: template_select_disabled,
                                    oninput: move |event| selected_template_slug.set(event.value()),
                                    match templates_state {
                                        Some(Ok(templates)) if templates.is_empty() => rsx! {
                                            option { value: "", "No templates" }
                                        },
                                        Some(Ok(templates)) => rsx! {
                                            option { value: "", "Blank page" }
                                            for template in templates {
                                                option {
                                                    key: "{template.slug}",
                                                    value: "{template.slug}",
                                                    "{template.title}"
                                                }
                                            }
                                        },
                                        Some(Err(_)) => rsx! {
                                            option { value: "", "Templates unavailable" }
                                        },
                                        None => rsx! {
                                            option { value: "", "Loading templates" }
                                        },
                                    }
                                }
                                button {
                                    class: "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400 disabled:cursor-not-allowed disabled:opacity-50",
                                    disabled: apply_template_disabled,
                                    onclick: move |event| on_apply_template.call(event),
                                    "Use"
                                }
                            }
                        }
                    }
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
                        "Categories"
                        input {
                            value: "{editor_categories}",
                            oninput: move |event| editor_categories.set(event.value()),
                            placeholder: "test-page, npc"
                        }
                    }
                    label { class: "flex items-center gap-2 text-sm font-semibold text-slate-700",
                        input {
                            class: "h-4 w-4 rounded border-stone-300 text-emerald-700",
                            r#type: "checkbox",
                            checked: editor_promoted(),
                            oninput: move |event| editor_promoted.set(event.checked())
                        }
                        span { "Promoted" }
                    }
                    div { class: "flex flex-wrap items-center gap-2",
                        ModeButton {
                            label: "Builder",
                            active: editor_mode() == EditorMode::Builder,
                            onclick: move |_| editor_mode.set(EditorMode::Builder)
                        }
                        ModeButton {
                            label: "Markdown",
                            active: editor_mode() == EditorMode::Markdown,
                            onclick: move |_| editor_mode.set(EditorMode::Markdown)
                        }
                    }
                    match editor_mode() {
                        EditorMode::Builder => rsx! {
                            PageBuilder { editor_markdown }
                        },
                        EditorMode::Markdown => rsx! {
                            label { class: "grid gap-1 text-sm font-semibold text-slate-700",
                                "Markdown"
                                textarea {
                                    value: "{editor_markdown}",
                                    oninput: move |event| editor_markdown.set(event.value()),
                                    placeholder: "# Page title"
                                }
                            }
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
                                    active_revision: selected_revision,
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
fn RevisionButton(revision: PageRevision, active_revision: Signal<String>) -> Element {
    let class = if revision.id == active_revision() {
        "w-full rounded-md border border-emerald-700 bg-emerald-50 p-3 text-left text-sm"
    } else {
        "w-full rounded-md border border-stone-200 bg-white p-3 text-left text-sm hover:border-stone-300"
    };

    rsx! {
        button {
            key: "{revision.id}",
            class,
            onclick: move |_| active_revision.set(revision.id.clone()),
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

#[cfg(feature = "server")]
async fn media_handler(
    dioxus::server::axum::extract::Path(path): dioxus::server::axum::extract::Path<String>,
) -> dioxus::server::axum::response::Response {
    use dioxus::server::axum::{
        http::{
            header::{CACHE_CONTROL, CONTENT_TYPE},
            HeaderValue, StatusCode,
        },
        response::IntoResponse,
    };

    match server::storage::read_media_file(&path) {
        Ok(Some(file)) => {
            let mut response = file.contents.into_response();
            response
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static(file.content_type));
            response.headers_mut().insert(
                CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=300"),
            );
            response
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[cfg(feature = "server")]
async fn export_handler(
    dioxus::server::axum::extract::Path(path): dioxus::server::axum::extract::Path<String>,
) -> dioxus::server::axum::response::Response {
    use dioxus::server::axum::{
        http::{
            header::{CACHE_CONTROL, CONTENT_TYPE},
            HeaderValue, StatusCode,
        },
        response::IntoResponse,
    };

    match server::storage::read_export_file(&path) {
        Ok(Some(file)) => {
            let mut response = file.contents.into_response();
            response
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static(file.content_type));
            response.headers_mut().insert(
                CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=300"),
            );
            response
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[get("/api/session/access", headers: dioxus::fullstack::HeaderMap)]
async fn current_user_access() -> ServerFnResult<UserAccess> {
    match server::auth::current_user_from_headers(&headers) {
        Some(user) => server::roles::access_for_user(&user).map_err(role_server_error),
        None => Ok(UserAccess {
            role: None,
            can_manage_users: false,
            can_manage_settings: false,
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

#[get("/api/templates")]
async fn list_page_templates() -> ServerFnResult<Vec<PageTemplateSummary>> {
    server::storage::list_templates().map_err(server_error)
}

#[get("/api/markdown-components")]
async fn list_markdown_component_manifests() -> ServerFnResult<Vec<String>> {
    server::storage::list_component_manifests().map_err(server_error)
}

#[post("/api/export/html", headers: dioxus::fullstack::HeaderMap)]
async fn export_wiki_html() -> ServerFnResult<HtmlExport> {
    let user = server::auth::current_user_from_headers(&headers).ok_or_else(|| {
        ServerFnError::ServerError {
            message: "sign in to export HTML".to_owned(),
            code: 401,
            details: None,
        }
    })?;

    server::roles::ensure_can_write_page(&user, "export").map_err(role_server_error)?;
    server::storage::export_html_site().map_err(server_error)
}

#[post("/api/templates/apply")]
async fn apply_page_template(
    template_slug: String,
    draft_slug: String,
    title: String,
) -> ServerFnResult<PageTemplateDraft> {
    server::storage::page_template_draft(&template_slug, &draft_slug, &title).map_err(server_error)
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

    validate_page_title(&title).map_err(client_validation_error)?;
    let normalized = normalize_page_slug(&slug).map_err(client_validation_error)?;
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

#[cfg(feature = "server")]
fn server_error(err: impl ToString) -> ServerFnError {
    server_error_with_code(err, 500)
}

#[cfg(feature = "server")]
fn client_validation_error(err: impl ToString) -> ServerFnError {
    server_error_with_code(err, 400)
}

#[cfg(feature = "server")]
fn server_error_with_code(err: impl ToString, code: u16) -> ServerFnError {
    ServerFnError::ServerError {
        message: err.to_string(),
        code,
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

fn app_server_url(path: &str) -> String {
    #[cfg(feature = "desktop")]
    {
        format!(
            "{}/{}",
            desktop_server_url().trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    #[cfg(not(feature = "desktop"))]
    {
        path.to_owned()
    }
}

fn auth_login_url(provider: &str) -> String {
    #[cfg(feature = "desktop")]
    {
        format!(
            "{}/auth/login/{provider}",
            desktop_server_url().trim_end_matches('/')
        )
    }

    #[cfg(not(feature = "desktop"))]
    {
        format!("/auth/login/{provider}")
    }
}

fn auth_logout_url() -> String {
    #[cfg(feature = "desktop")]
    {
        format!("{}/auth/logout", desktop_server_url().trim_end_matches('/'))
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

#[cfg(test)]
mod route_tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn page_route_should_parse_slug_from_root_segment() {
        let route = Route::from_str("/guide").expect("page route should parse");

        assert_eq!(
            route,
            Route::Page {
                slug: "guide".to_owned()
            }
        );
    }

    #[test]
    fn root_route_should_redirect_to_home_page() {
        let route = Route::from_str("/").expect("root route should parse");

        assert_eq!(
            route,
            Route::Page {
                slug: "home".to_owned()
            }
        );
    }

    #[test]
    fn prepare_page_save_should_normalize_title_when_slug_is_empty() {
        let input = prepare_page_save("Getting Started", "  ", "guide", true, "# Body")
            .expect("page input should be prepared");

        assert_eq!(input.slug, "getting-started");
        assert_eq!(input.title, "Getting Started");
        assert!(input.markdown.contains("categories: [guide]"));
    }

    #[test]
    fn prepare_page_save_should_reject_invalid_title() {
        let result = prepare_page_save("  ", "guide", "", true, "# Body");

        assert_eq!(result, Err(slug::PageValidationError::EmptyTitle));
    }
}
