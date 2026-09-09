//! The standalone desktop process operates as the local wiki owner.
use crate::{models::AuthProviderInfo, user::AuthUser};

pub fn current_user_from_headers(_: &dioxus::fullstack::HeaderMap) -> Option<AuthUser> {
    Some(AuthUser {
        id: crate::desktop::LOCAL_USER_ID.to_owned(),
        name: "Local owner".to_owned(),
        email: None,
    })
}

pub fn configured_providers() -> Vec<AuthProviderInfo> {
    Vec::new()
}
