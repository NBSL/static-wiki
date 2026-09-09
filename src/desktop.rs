//! Local webview assets are read in-process; no HTTP listener is needed.
#[cfg(any(feature = "desktop", test))]
use crate::server::storage;
#[cfg(feature = "desktop")]
use dioxus::desktop::{use_asset_handler, wry::http::Response};

pub const LOCAL_USER_ID: &str = "desktop:local-owner";

#[cfg(feature = "desktop")]
pub fn use_local_assets() {
    use_asset_handler("media", |request, responder| {
        let result = storage::read_media_file(request.uri().path().trim_start_matches("/media/"))
            .map(|file| file.map(|file| (file.contents, file.content_type)));
        responder.respond(asset_response(result));
    });
    use_asset_handler("exports", |request, responder| {
        let result =
            storage::read_export_file(request.uri().path().trim_start_matches("/exports/"))
                .map(|file| file.map(|file| (file.contents, file.content_type)));
        responder.respond(asset_response(result));
    });
}

#[cfg(feature = "desktop")]
fn asset_response(
    result: storage::StorageResult<Option<(Vec<u8>, &'static str)>>,
) -> Response<Vec<u8>> {
    match result {
        Ok(Some((contents, content_type))) => Response::builder()
            .header("Content-Type", content_type)
            .body(contents)
            .unwrap(),
        Ok(None) => Response::builder().status(404).body(Vec::new()).unwrap(),
        Err(error) => Response::builder()
            .status(500)
            .body(error.to_string().into_bytes())
            .unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };

    // Local operations must complete without an HTTP client or async runtime.
    fn local<T>(future: impl Future<Output = dioxus::prelude::ServerFnResult<T>>) -> T {
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(result) => result.expect("local operation should succeed"),
            Poll::Pending => panic!("local operation unexpectedly waited on I/O"),
        }
    }

    #[test]
    fn desktop_works_without_a_backend() {
        // Isolate configuration from the user's wiki and other tests.
        if std::env::var_os("XP_WIKI_DESKTOP_TEST_CHILD").is_none() {
            let dir = tempfile::tempdir().unwrap();
            let env_file = dir.path().join(".env");
            std::fs::write(
                &env_file,
                format!(
                    "XP_WIKI_DATA_DIR={}\nXP_WIKI_DEFAULT_ROLE=none\n",
                    dir.path().join("wiki").display()
                ),
            )
            .unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "desktop::tests::desktop_works_without_a_backend",
                    "--nocapture",
                ])
                .env("XP_WIKI_DESKTOP_TEST_CHILD", "1")
                .env("XP_WIKI_ENV_FILE", env_file)
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }

        assert_eq!(local(crate::current_user()).unwrap().id, LOCAL_USER_ID);
        assert_eq!(
            local(crate::current_user_access()).role.as_deref(),
            Some("admin")
        );
        assert!(local(crate::configured_oauth_providers()).is_empty());
        assert!(local(crate::get_wiki_page("home".into())).is_some());
        local(crate::save_wiki_page(
            "offline".into(),
            "Offline".into(),
            "First version".into(),
        ));
        local(crate::save_wiki_page(
            "offline".into(),
            "Offline".into(),
            "Second version".into(),
        ));
        assert!(local(crate::get_wiki_page("offline".into()))
            .unwrap()
            .markdown
            .contains("Second version"));
        assert!(local(crate::list_wiki_pages())
            .iter()
            .any(|page| page.slug == "offline"));
        let history = local(crate::get_wiki_history("offline".into()));
        assert_eq!(history.len(), 2);
        local(crate::get_wiki_diff(
            "offline".into(),
            history[0].id.clone(),
        ));
        assert!(!local(crate::list_page_templates()).is_empty());
        assert!(!local(crate::list_markdown_component_manifests()).is_empty());
        local(crate::media::list_media_entries(String::new()));
        local(crate::settings::load_settings_overview());
        local(crate::user::list_managed_users());
        local(crate::export_wiki_html());
        let exported = storage::read_export_file("latest/offline.html")
            .unwrap()
            .unwrap();
        assert!(String::from_utf8(exported.contents)
            .unwrap()
            .contains("Second version"));
        let owner = local(crate::current_user()).unwrap();
        storage::save_media_file("", "offline.png", b"local image", &owner).unwrap();
        let media = storage::read_media_file("offline.png").unwrap().unwrap();
        assert_eq!(media.contents, b"local image");
        assert_eq!(media.content_type, "image/png");
    }
}
