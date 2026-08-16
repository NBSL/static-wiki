use crate::models::{MediaEntry, MediaEntryKind, MediaListing};
#[cfg(feature = "server")]
use crate::user::AuthUser;
use dioxus::prelude::*;

#[component]
pub(crate) fn MediaManager(
    media_state: Option<ServerFnResult<MediaListing>>,
    mut media_path: Signal<String>,
    mut new_folder_name: Signal<String>,
    can_manage_media: bool,
    mut refresh_key: Signal<u64>,
    mut status: Signal<String>,
) -> Element {
    let mut upload_busy = use_signal(|| false);
    let mut dragged_media_path = use_signal(String::new);
    let mut pending_delete_path = use_signal(String::new);
    let current_path = media_path();
    let breadcrumbs = media_breadcrumbs(&current_path);
    let folder_name = new_folder_name();
    let create_disabled = !can_manage_media || folder_name.trim().is_empty();
    let upload_disabled = !can_manage_media || upload_busy();

    let create_folder_action = move |_| async move {
        let parent = media_path();
        let name = new_folder_name();
        if name.trim().is_empty() {
            status.set("Choose a folder name.".to_owned());
            return;
        }

        match create_media_folder(parent, name).await {
            Ok(listing) => {
                media_path.set(listing.path);
                new_folder_name.set(String::new());
                refresh_key.set(refresh_key() + 1);
                status.set("Folder created.".to_owned());
            }
            Err(err) => status.set(format!("Folder create failed: {err}")),
        }
    };

    let upload_files_action = move |event: dioxus::events::FormEvent| async move {
        if !can_manage_media {
            status.set("Sign in as an editor or admin to upload media.".to_owned());
            return;
        }

        let files = event.files();
        if files.is_empty() {
            return;
        }

        upload_busy.set(true);
        let folder = media_path();
        let total = files.len();
        let mut uploaded = 0_usize;
        let mut errors = Vec::new();

        for file in files {
            let filename = file.name();
            match file.read_bytes().await {
                Ok(bytes) => {
                    match upload_media_file(folder.clone(), filename.clone(), bytes.to_vec()).await
                    {
                        Ok(_) => uploaded += 1,
                        Err(err) => errors.push(format!("{filename}: {err}")),
                    }
                }
                Err(err) => errors.push(format!("{filename}: {err}")),
            }
        }

        upload_busy.set(false);
        refresh_key.set(refresh_key() + 1);
        status.set(upload_status(uploaded, total, &errors));
    };

    let move_media_action = move |request: MediaMoveRequest| async move {
        if !can_manage_media {
            status.set("Sign in as an editor or admin to move media.".to_owned());
            return;
        }
        if request.source_path.trim().is_empty() {
            return;
        }

        match move_media_file(request.source_path.clone(), request.target_folder.clone()).await {
            Ok(()) => {
                dragged_media_path.set(String::new());
                refresh_key.set(refresh_key() + 1);
                status.set(format!(
                    "Moved {} to {}.",
                    media_file_name(&request.source_path),
                    media_folder_label(&request.target_folder)
                ));
            }
            Err(err) => status.set(format!("Move failed: {err}")),
        }
    };

    let delete_media_action = move |request: MediaDeleteRequest| async move {
        if !can_manage_media {
            status.set("Sign in as an editor or admin to delete media.".to_owned());
            return;
        }
        if request.path.trim().is_empty() {
            return;
        }

        match delete_media_entry(request.path.clone()).await {
            Ok(()) => {
                pending_delete_path.set(String::new());
                refresh_key.set(refresh_key() + 1);
                status.set(format!(
                    "Deleted {}.",
                    media_delete_target_label(&request.path, request.kind)
                ));
            }
            Err(err) => status.set(format!("Delete failed: {err}")),
        }
    };

    rsx! {
        div { class: "grid gap-4 lg:grid-cols-[minmax(0,1fr)_320px]",
            section { class: "rounded-lg border border-stone-200 bg-white p-4 shadow-sm",
                div { class: "mb-4 flex flex-wrap items-center justify-between gap-3",
                    div { class: "min-w-0",
                        h3 { class: "text-base font-semibold text-slate-950", "Media Library" }
                        div { class: "mt-2 flex flex-wrap items-center gap-1 text-sm",
                            for crumb in breadcrumbs {
                                MediaBreadcrumb {
                                    key: "{crumb.path}",
                                    crumb,
                                    current_path: current_path.clone(),
                                    can_manage_media,
                                    dragged_media_path,
                                    on_select: move |path: String| {
                                        media_path.set(path);
                                        status.set(String::new());
                                    },
                                    on_move_to_folder: move_media_action,
                                }
                            }
                        }
                    }
                    button {
                        class: "inline-flex h-9 items-center rounded-md border border-stone-300 bg-white px-3 text-sm font-semibold text-slate-800 hover:border-stone-400",
                        onclick: move |_| {
                            refresh_key.set(refresh_key() + 1);
                        },
                        "Refresh"
                    }
                }

                match media_state {
                    Some(Ok(listing)) if listing.entries.is_empty() => rsx! {
                        div { class: "rounded-lg border border-dashed border-stone-300 bg-stone-50 p-8 text-sm text-slate-500",
                            "No media in this folder"
                        }
                    },
                    Some(Ok(listing)) => rsx! {
                        div { class: "grid gap-3 sm:grid-cols-2 xl:grid-cols-3",
                            for entry in listing.entries {
                                MediaEntryTile {
                                    key: "{entry.path}",
                                    entry,
                                    can_manage_media,
                                    dragged_media_path,
                                    pending_delete_path,
                                    on_open_folder: move |path: String| {
                                        media_path.set(path);
                                        status.set(String::new());
                                    },
                                    on_move_to_folder: move_media_action,
                                    on_delete: delete_media_action,
                                }
                            }
                        }
                    },
                    Some(Err(err)) => rsx! {
                        p { class: "rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-800", "{err}" }
                    },
                    None => rsx! {
                        p { class: "py-6 text-sm text-slate-500", "Loading media" }
                    },
                }
            }

            section { class: "rounded-lg border border-stone-200 bg-white p-4 shadow-sm",
                h3 { class: "mb-3 text-base font-semibold text-slate-950", "Manage Media" }
                if can_manage_media {
                    div { class: "grid gap-4",
                        label { class: "grid gap-1 text-sm font-semibold text-slate-700",
                            "New Folder"
                            div { class: "flex gap-2",
                                input {
                                    value: "{folder_name}",
                                    oninput: move |event| new_folder_name.set(event.value()),
                                    placeholder: "folder-name"
                                }
                                button {
                                    class: "inline-flex h-10 shrink-0 items-center rounded-md border border-emerald-700 bg-emerald-700 px-3 text-sm font-semibold text-white disabled:cursor-not-allowed disabled:opacity-50",
                                    disabled: create_disabled,
                                    onclick: create_folder_action,
                                    "Create"
                                }
                            }
                        }

                        label { class: "grid gap-1 text-sm font-semibold text-slate-700",
                            "Upload"
                            input {
                                r#type: "file",
                                accept: "image/*,video/*",
                                multiple: true,
                                disabled: upload_disabled,
                                onchange: upload_files_action
                            }
                        }
                        p { class: "text-xs text-slate-500",
                            "Images and videos up to 50 MiB are saved under /media/{current_path}"
                        }
                    }
                } else {
                    p { class: "rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-900",
                        "Sign in as an editor or admin to manage media."
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct MediaMoveRequest {
    source_path: String,
    target_folder: String,
}

#[derive(Clone, Debug, PartialEq)]
struct MediaDeleteRequest {
    path: String,
    kind: MediaEntryKind,
}

#[derive(Clone, Debug, PartialEq)]
struct MediaCrumb {
    label: String,
    path: String,
}

#[component]
fn MediaBreadcrumb(
    crumb: MediaCrumb,
    current_path: String,
    can_manage_media: bool,
    mut dragged_media_path: Signal<String>,
    on_select: EventHandler<String>,
    on_move_to_folder: EventHandler<MediaMoveRequest>,
) -> Element {
    let is_current = crumb.path == current_path;
    let path = crumb.path.clone();
    let drop_path = crumb.path.clone();

    rsx! {
        if is_current {
            span { class: "rounded-md bg-slate-100 px-2 py-1 font-semibold text-slate-800", "{crumb.label}" }
        } else {
            button {
                class: "rounded-md px-2 py-1 font-semibold text-emerald-800 hover:bg-emerald-50",
                onclick: move |_| on_select.call(path.clone()),
                ondragover: move |event: dioxus::events::DragEvent| {
                    if can_manage_media {
                        event.prevent_default();
                        event.data_transfer().set_drop_effect("move");
                    }
                },
                ondrop: move |event: dioxus::events::DragEvent| {
                    let target_folder = drop_path.clone();
                    async move {
                        if !can_manage_media {
                            return;
                        }

                        event.prevent_default();
                        let source_path = event
                            .data_transfer()
                            .get_as_text()
                            .unwrap_or_else(|| dragged_media_path());
                        if source_path.trim().is_empty() {
                            return;
                        }

                        on_move_to_folder.call(MediaMoveRequest {
                            source_path,
                            target_folder,
                        });
                    }
                },
                "{crumb.label}"
            }
        }
    }
}

#[component]
fn MediaEntryTile(
    entry: MediaEntry,
    can_manage_media: bool,
    mut dragged_media_path: Signal<String>,
    pending_delete_path: Signal<String>,
    on_open_folder: EventHandler<String>,
    on_move_to_folder: EventHandler<MediaMoveRequest>,
    on_delete: EventHandler<MediaDeleteRequest>,
) -> Element {
    let kind_label = media_kind_label(entry.kind);
    let size_label = format_media_size(entry.size);
    let path_label = match &entry.url {
        Some(url) => url.clone(),
        None => entry.path.clone(),
    };

    match entry.kind {
        MediaEntryKind::Folder => {
            let path = entry.path.clone();
            let drop_path = entry.path.clone();
            rsx! {
                div {
                    class: folder_tile_class(can_manage_media),
                    ondragover: move |event: dioxus::events::DragEvent| {
                        if can_manage_media {
                            event.prevent_default();
                            event.data_transfer().set_drop_effect("move");
                        }
                    },
                    ondrop: move |event: dioxus::events::DragEvent| {
                        let target_folder = drop_path.clone();
                        async move {
                            if !can_manage_media {
                                return;
                            }

                            event.prevent_default();
                            let source_path = event
                                .data_transfer()
                                .get_as_text()
                                .unwrap_or_else(|| dragged_media_path());
                            if source_path.trim().is_empty() {
                                return;
                            }

                            on_move_to_folder.call(MediaMoveRequest {
                                source_path,
                                target_folder,
                            });
                        }
                    },
                    button {
                        class: "grid w-full gap-3 text-left",
                        onclick: move |_| on_open_folder.call(path.clone()),
                        div { class: "flex min-h-32 items-center justify-center rounded-md bg-stone-50 text-sm font-semibold text-slate-600",
                            "Folder"
                        }
                        div { class: "min-w-0",
                            span { class: "block truncate text-sm font-semibold text-slate-950", "{entry.name}" }
                            span { class: "block truncate text-xs text-slate-500", "{entry.path}" }
                        }
                    }
                    MediaEntryActions {
                        entry_path: entry.path,
                        entry_kind: entry.kind,
                        can_manage_media,
                        pending_delete_path,
                        on_delete,
                    }
                }
            }
        }
        MediaEntryKind::Image => {
            let url = entry.url.clone().unwrap_or_default();
            let drag_path = entry.path.clone();
            rsx! {
                div {
                    class: media_file_tile_class(can_manage_media),
                    draggable: can_manage_media,
                    ondragstart: move |event: dioxus::events::DragEvent| {
                        if can_manage_media {
                            dragged_media_path.set(drag_path.clone());
                            let transfer = event.data_transfer();
                            let _ = transfer.set_data("text/plain", &drag_path);
                            transfer.set_effect_allowed("move");
                        }
                    },
                    ondragend: move |_| dragged_media_path.set(String::new()),
                    div { class: "flex min-h-32 items-center justify-center overflow-hidden rounded-md bg-stone-50",
                        img {
                            class: "max-h-48 w-full object-contain",
                            src: "{url}",
                            alt: "{entry.name}"
                        }
                    }
                    MediaEntryMeta {
                        name: entry.name.clone(),
                        kind_label,
                        size_label,
                        path_label
                    }
                    MediaEntryActions {
                        entry_path: entry.path,
                        entry_kind: entry.kind,
                        can_manage_media,
                        pending_delete_path,
                        on_delete,
                    }
                }
            }
        }
        MediaEntryKind::Video => {
            let url = entry.url.clone().unwrap_or_default();
            let drag_path = entry.path.clone();
            rsx! {
                div {
                    class: media_file_tile_class(can_manage_media),
                    draggable: can_manage_media,
                    ondragstart: move |event: dioxus::events::DragEvent| {
                        if can_manage_media {
                            dragged_media_path.set(drag_path.clone());
                            let transfer = event.data_transfer();
                            let _ = transfer.set_data("text/plain", &drag_path);
                            transfer.set_effect_allowed("move");
                        }
                    },
                    ondragend: move |_| dragged_media_path.set(String::new()),
                    div { class: "overflow-hidden rounded-md bg-black",
                        video {
                            class: "max-h-48 w-full",
                            controls: true,
                            preload: "metadata",
                            src: "{url}"
                        }
                    }
                    MediaEntryMeta {
                        name: entry.name,
                        kind_label,
                        size_label,
                        path_label
                    }
                    MediaEntryActions {
                        entry_path: entry.path,
                        entry_kind: entry.kind,
                        can_manage_media,
                        pending_delete_path,
                        on_delete,
                    }
                }
            }
        }
    }
}

fn folder_tile_class(can_manage_media: bool) -> &'static str {
    if can_manage_media {
        "grid min-h-48 w-full gap-3 rounded-lg border border-stone-200 bg-white p-3 text-left hover:border-emerald-700"
    } else {
        "grid min-h-48 w-full gap-3 rounded-lg border border-stone-200 bg-white p-3 text-left hover:border-stone-300"
    }
}

fn media_file_tile_class(can_manage_media: bool) -> &'static str {
    if can_manage_media {
        "grid min-h-48 cursor-grab gap-3 rounded-lg border border-stone-200 bg-white p-3 active:cursor-grabbing"
    } else {
        "grid min-h-48 gap-3 rounded-lg border border-stone-200 bg-white p-3"
    }
}

#[component]
fn MediaEntryActions(
    entry_path: String,
    entry_kind: MediaEntryKind,
    can_manage_media: bool,
    mut pending_delete_path: Signal<String>,
    on_delete: EventHandler<MediaDeleteRequest>,
) -> Element {
    if !can_manage_media {
        return rsx! {};
    }

    let is_pending_delete = pending_delete_path() == entry_path;
    let confirm_path = entry_path.clone();
    let cancel_path = entry_path.clone();

    rsx! {
        div { class: "flex flex-wrap justify-end gap-2",
            if is_pending_delete {
                button {
                    class: "inline-flex h-8 items-center rounded-md border border-red-700 bg-red-700 px-2 text-xs font-semibold text-white hover:bg-red-800",
                    onclick: move |_| {
                        on_delete.call(MediaDeleteRequest {
                            path: confirm_path.clone(),
                            kind: entry_kind,
                        });
                    },
                    "Confirm"
                }
                button {
                    class: "inline-flex h-8 items-center rounded-md border border-stone-300 bg-white px-2 text-xs font-semibold text-slate-800 hover:border-stone-400",
                    onclick: move |_| {
                        pending_delete_path.set(String::new());
                    },
                    "Cancel"
                }
            } else {
                button {
                    class: "inline-flex h-8 items-center rounded-md border border-red-200 bg-white px-2 text-xs font-semibold text-red-700 hover:border-red-300",
                    onclick: move |_| {
                        pending_delete_path.set(cancel_path.clone());
                    },
                    "Delete"
                }
            }
        }
    }
}

#[component]
fn MediaEntryMeta(
    name: String,
    kind_label: &'static str,
    size_label: String,
    path_label: String,
) -> Element {
    rsx! {
        div { class: "min-w-0",
            div { class: "mb-1 flex flex-wrap items-center gap-2",
                span { class: "truncate text-sm font-semibold text-slate-950", "{name}" }
                span { class: "rounded-md bg-slate-100 px-2 py-1 text-xs font-semibold text-slate-600", "{kind_label}" }
                if !size_label.is_empty() {
                    span { class: "text-xs text-slate-500", "{size_label}" }
                }
            }
            a {
                class: "block truncate font-mono text-xs text-emerald-800 hover:underline",
                href: "{path_label}",
                target: "_blank",
                rel: "noreferrer",
                "{path_label}"
            }
        }
    }
}

fn media_breadcrumbs(path: &str) -> Vec<MediaCrumb> {
    let mut crumbs = vec![MediaCrumb {
        label: "Media".to_owned(),
        path: String::new(),
    }];
    let mut current = Vec::new();

    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        current.push(segment.to_owned());
        crumbs.push(MediaCrumb {
            label: segment.to_owned(),
            path: current.join("/"),
        });
    }

    crumbs
}

fn media_kind_label(kind: MediaEntryKind) -> &'static str {
    match kind {
        MediaEntryKind::Folder => "Folder",
        MediaEntryKind::Image => "Image",
        MediaEntryKind::Video => "Video",
    }
}

fn format_media_size(size: Option<u64>) -> String {
    let Some(size) = size else {
        return String::new();
    };

    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let size = size as f64;

    if size >= GB {
        format!("{:.1} GB", size / GB)
    } else if size >= MB {
        format!("{:.1} MB", size / MB)
    } else if size >= KB {
        format!("{:.1} KB", size / KB)
    } else {
        format!("{size:.0} B")
    }
}

fn upload_status(uploaded: usize, total: usize, errors: &[String]) -> String {
    if uploaded == total {
        return match uploaded {
            1 => "Uploaded 1 file.".to_owned(),
            count => format!("Uploaded {count} files."),
        };
    }

    let first_error = errors
        .first()
        .map(String::as_str)
        .unwrap_or("Unknown upload error");
    if uploaded == 0 {
        format!("Upload failed: {first_error}")
    } else {
        format!("Uploaded {uploaded} of {total} files. First error: {first_error}")
    }
}

fn media_file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn media_folder_label(path: &str) -> String {
    if path.is_empty() {
        "Media".to_owned()
    } else {
        path.to_owned()
    }
}

fn media_delete_target_label(path: &str, kind: MediaEntryKind) -> String {
    match kind {
        MediaEntryKind::Folder => format!("folder {}", media_file_name(path)),
        MediaEntryKind::Image | MediaEntryKind::Video => media_file_name(path).to_owned(),
    }
}

#[post("/api/media/list")]
pub(crate) async fn list_media_entries(path: String) -> ServerFnResult<MediaListing> {
    crate::server::storage::list_media(&path).map_err(media_storage_error)
}

#[post("/api/media/folders", headers: dioxus::fullstack::HeaderMap)]
async fn create_media_folder(parent: String, name: String) -> ServerFnResult<MediaListing> {
    let user = authenticated_user_from_headers(&headers)?;
    crate::server::roles::ensure_can_write_page(&user, "media").map_err(role_server_error)?;
    crate::server::storage::create_media_folder(&parent, &name, &user).map_err(media_storage_error)
}

#[post("/api/media/upload", headers: dioxus::fullstack::HeaderMap)]
async fn upload_media_file(
    folder: String,
    filename: String,
    contents: Vec<u8>,
) -> ServerFnResult<MediaListing> {
    let user = authenticated_user_from_headers(&headers)?;
    crate::server::roles::ensure_can_write_page(&user, "media").map_err(role_server_error)?;
    crate::server::storage::save_media_file(&folder, &filename, &contents, &user)
        .map_err(media_storage_error)
}

#[post("/api/media/move", headers: dioxus::fullstack::HeaderMap)]
async fn move_media_file(source_path: String, target_folder: String) -> ServerFnResult<()> {
    let user = authenticated_user_from_headers(&headers)?;
    crate::server::roles::ensure_can_write_page(&user, "media").map_err(role_server_error)?;
    crate::server::storage::move_media_file(&source_path, &target_folder, &user)
        .map_err(media_storage_error)
}

#[post("/api/media/delete", headers: dioxus::fullstack::HeaderMap)]
async fn delete_media_entry(path: String) -> ServerFnResult<()> {
    let user = authenticated_user_from_headers(&headers)?;
    crate::server::roles::ensure_can_write_page(&user, "media").map_err(role_server_error)?;
    crate::server::storage::delete_media_entry(&path, &user).map_err(media_storage_error)
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
fn media_storage_error(err: crate::server::storage::StorageError) -> ServerFnError {
    let code = match err {
        crate::server::storage::StorageError::InvalidMediaPath(_)
        | crate::server::storage::StorageError::UnsupportedMediaType(_) => 400,
        crate::server::storage::StorageError::MediaFileTooLarge { .. } => 413,
        crate::server::storage::StorageError::MediaEntryNotFound(_)
        | crate::server::storage::StorageError::MediaFileNotFound(_)
        | crate::server::storage::StorageError::MediaFolderNotFound(_) => 404,
        crate::server::storage::StorageError::MediaFileExists(_)
        | crate::server::storage::StorageError::MediaFolderExists(_) => 409,
        _ => 500,
    };

    ServerFnError::ServerError {
        message: err.to_string(),
        code,
        details: None,
    }
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
