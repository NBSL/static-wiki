use crate::models::{AuthProviderInfo, AuthUser};
use crate::server::env::{load_dotenv, optional_env};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use dioxus::server::axum::{
    extract::{Path, Query},
    http::{
        header::{COOKIE, LOCATION, SET_COOKIE, USER_AGENT},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
};
use oauth2::{
    basic::BasicClient, AuthType, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;

const SESSION_COOKIE: &str = "xp_wiki_session";
const PENDING_LOGIN_COOKIE: &str = "xp_wiki_oauth_pending";
const PENDING_LOGIN_MAX_AGE_SECONDS: u16 = 600;
const UI_URL_ENV: &str = "XP_WIKI_UI_URL";
const DEFAULT_UI_REDIRECT_URL: &str = "/";

static PENDING_LOGINS: Lazy<Mutex<HashMap<String, PendingLogin>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static SESSIONS: Lazy<Mutex<HashMap<String, AuthUser>>> = Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Deserialize, Serialize)]
struct PendingLogin {
    provider: String,
    state: String,
    pkce_verifier: String,
}

struct OAuthLogin {
    auth_url: String,
    pending_login: PendingLogin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OAuthProvider {
    GitHub,
    Google,
    Discord,
}

#[derive(Clone)]
struct OAuthConfig {
    client_id: String,
    client_secret: String,
    authorize_url: String,
    token_url: String,
    redirect_url: String,
    userinfo_url: String,
    scopes: Vec<String>,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("missing environment variable `{0}`")]
    MissingEnv(String),
    #[error("OAuth URL is invalid: {0}")]
    InvalidUrl(String),
    #[error("OAuth login state was not found")]
    MissingState,
    #[error("OAuth login state did not match the callback")]
    StateMismatch,
    #[error("OAuth login state cookie was invalid")]
    InvalidStateCookie,
    #[error("OAuth provider returned an error: {0}")]
    Provider(String),
    #[error("OAuth token exchange failed: {0}")]
    Token(String),
    #[error("OAuth user lookup failed: {0}")]
    UserInfo(String),
    #[error("authentication state lock was poisoned")]
    Lock,
    #[error("unknown OAuth provider `{0}`")]
    UnknownProvider(String),
    #[error("OAuth callback provider did not match login provider")]
    ProviderMismatch,
}

#[derive(Deserialize)]
pub struct AuthCallback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct ProviderUser {
    id: Option<serde_json::Value>,
    sub: Option<String>,
    login: Option<String>,
    name: Option<String>,
    email: Option<String>,
    username: Option<String>,
    preferred_username: Option<String>,
    global_name: Option<String>,
}

pub async fn login_handler(Path(provider): Path<String>) -> Response {
    match authorize_redirect(&provider) {
        Ok(login) => {
            let mut response = redirect_response(&login.auth_url);
            match pending_login_cookie(&login.pending_login) {
                Ok(cookie) => append_set_cookie(&mut response, cookie),
                Err(err) => {
                    return error_response(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string());
                }
            }
            response
        }
        Err(err) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
    }
}

pub async fn callback_handler(
    Path(provider): Path<String>,
    Query(callback): Query<AuthCallback>,
    headers: HeaderMap,
) -> Response {
    let mut response = match complete_login(&provider, callback, &headers).await {
        Ok(user) => {
            if let Err(err) = crate::server::roles::record_authenticated_user(&user) {
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string());
            }

            let session_id = Uuid::new_v4().to_string();
            let Ok(mut sessions) = SESSIONS.lock() else {
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, "session lock failed");
            };
            sessions.insert(session_id.clone(), user);

            let ui_url = post_login_redirect_url();
            let mut response = redirect_response(&ui_url);
            let cookie = format!(
                "{SESSION_COOKIE}={session_id}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000"
            );
            append_set_cookie(&mut response, cookie);
            response
        }
        Err(err) => error_response(StatusCode::UNAUTHORIZED, &err.to_string()),
    };
    append_set_cookie(&mut response, clear_pending_login_cookie());
    response
}

pub async fn logout_handler(headers: HeaderMap) -> Response {
    if let Some(session_id) = session_id_from_headers(&headers) {
        if let Ok(mut sessions) = SESSIONS.lock() {
            sessions.remove(&session_id);
        }
    }

    let mut response = redirect_response("/");
    let cookie = format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    append_set_cookie(&mut response, cookie);
    response
}

pub fn current_user_from_headers(headers: &HeaderMap) -> Option<AuthUser> {
    let session_id = session_id_from_headers(headers)?;
    SESSIONS.lock().ok()?.get(&session_id).cloned()
}

pub fn configured_providers() -> Vec<AuthProviderInfo> {
    load_env();
    OAuthProvider::all()
        .into_iter()
        .filter(|provider| OAuthConfig::is_configured(*provider))
        .map(|provider| AuthProviderInfo {
            slug: provider.slug().to_owned(),
            label: provider.label().to_owned(),
        })
        .collect()
}

fn authorize_redirect(provider: &str) -> Result<OAuthLogin, AuthError> {
    let provider = OAuthProvider::from_slug(provider)?;
    let config = OAuthConfig::from_env(provider)?;
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let client = BasicClient::new(ClientId::new(config.client_id.clone()))
        .set_client_secret(ClientSecret::new(config.client_secret.clone()))
        .set_auth_type(AuthType::RequestBody)
        .set_auth_uri(AuthUrl::new(config.authorize_url.clone()).map_err(invalid_url)?)
        .set_token_uri(TokenUrl::new(config.token_url.clone()).map_err(invalid_url)?)
        .set_redirect_uri(RedirectUrl::new(config.redirect_url.clone()).map_err(invalid_url)?);

    let mut request = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge);

    for scope in &config.scopes {
        request = request.add_scope(Scope::new(scope.clone()));
    }

    let (auth_url, csrf_token) = request.url();
    let pending_login = PendingLogin {
        provider: provider.slug().to_owned(),
        state: csrf_token.secret().to_owned(),
        pkce_verifier: pkce_verifier.secret().to_owned(),
    };
    PENDING_LOGINS
        .lock()
        .map_err(|_| AuthError::Lock)?
        .insert(pending_login.state.clone(), pending_login.clone());

    Ok(OAuthLogin {
        auth_url: auth_url.to_string(),
        pending_login,
    })
}

async fn complete_login(
    provider: &str,
    callback: AuthCallback,
    headers: &HeaderMap,
) -> Result<AuthUser, AuthError> {
    let provider = OAuthProvider::from_slug(provider)?;
    if let Some(error) = callback.error {
        return Err(AuthError::Provider(error));
    }

    let state = callback.state.ok_or(AuthError::MissingState)?;
    let code = callback
        .code
        .ok_or_else(|| AuthError::Provider("missing authorization code".to_owned()))?;
    let pending = match take_pending_login(&state)? {
        Some(pending) => pending,
        None => pending_login_from_headers(headers)?.ok_or(AuthError::MissingState)?,
    };

    if pending.state != state {
        return Err(AuthError::StateMismatch);
    }

    if pending.provider != provider.slug() {
        return Err(AuthError::ProviderMismatch);
    }

    let config = OAuthConfig::from_env(provider)?;
    let client = BasicClient::new(ClientId::new(config.client_id.clone()))
        .set_client_secret(ClientSecret::new(config.client_secret.clone()))
        .set_auth_type(AuthType::RequestBody)
        .set_auth_uri(AuthUrl::new(config.authorize_url.clone()).map_err(invalid_url)?)
        .set_token_uri(TokenUrl::new(config.token_url.clone()).map_err(invalid_url)?)
        .set_redirect_uri(RedirectUrl::new(config.redirect_url.clone()).map_err(invalid_url)?);

    let http_client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| AuthError::Token(err.to_string()))?;

    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(PkceCodeVerifier::new(pending.pkce_verifier))
        .request_async(&http_client)
        .await
        .map_err(|err| AuthError::Token(err.to_string()))?;

    fetch_user(&config, token.access_token().secret()).await
}

async fn fetch_user(config: &OAuthConfig, access_token: &str) -> Result<AuthUser, AuthError> {
    let provider_user = reqwest::Client::new()
        .get(&config.userinfo_url)
        .bearer_auth(access_token)
        .header(USER_AGENT.as_str(), "xp-static-wiki")
        .send()
        .await
        .map_err(|err| AuthError::UserInfo(err.to_string()))?
        .error_for_status()
        .map_err(|err| AuthError::UserInfo(err.to_string()))?
        .json::<ProviderUser>()
        .await
        .map_err(|err| AuthError::UserInfo(err.to_string()))?;

    Ok(provider_user.into_auth_user())
}

impl ProviderUser {
    fn into_auth_user(self) -> AuthUser {
        let id = self
            .sub
            .or_else(|| self.id.as_ref().map(json_value_to_string))
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let name = self
            .name
            .or(self.login)
            .or(self.global_name)
            .or(self.username)
            .or(self.preferred_username)
            .unwrap_or_else(|| id.clone());

        AuthUser {
            id,
            name,
            email: self.email,
        }
    }
}

impl OAuthConfig {
    fn from_env(provider: OAuthProvider) -> Result<Self, AuthError> {
        load_env();
        let prefix = provider.env_prefix();
        let scopes = optional_env(&format!("{prefix}_SCOPES"))
            .unwrap_or_else(|| provider.default_scopes().to_owned())
            .split([',', ' '])
            .filter(|scope| !scope.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        Ok(Self {
            client_id: required_env(&format!("{prefix}_CLIENT_ID"))?,
            client_secret: required_env(&format!("{prefix}_CLIENT_SECRET"))?,
            authorize_url: optional_env(&format!("{prefix}_AUTHORIZE_URL"))
                .unwrap_or_else(|| provider.default_authorize_url().to_owned()),
            token_url: optional_env(&format!("{prefix}_TOKEN_URL"))
                .unwrap_or_else(|| provider.default_token_url().to_owned()),
            redirect_url: optional_env(&format!("{prefix}_REDIRECT_URL"))
                .unwrap_or_else(|| provider.default_redirect_url()),
            userinfo_url: optional_env(&format!("{prefix}_USERINFO_URL"))
                .unwrap_or_else(|| provider.default_userinfo_url().to_owned()),
            scopes,
        })
    }

    fn is_configured(provider: OAuthProvider) -> bool {
        let prefix = provider.env_prefix();
        optional_env(&format!("{prefix}_CLIENT_ID")).is_some()
            && optional_env(&format!("{prefix}_CLIENT_SECRET")).is_some()
    }
}

impl OAuthProvider {
    fn all() -> [Self; 3] {
        [Self::GitHub, Self::Google, Self::Discord]
    }

    fn from_slug(slug: &str) -> Result<Self, AuthError> {
        match slug {
            "github" => Ok(Self::GitHub),
            "google" => Ok(Self::Google),
            "discord" => Ok(Self::Discord),
            other => Err(AuthError::UnknownProvider(other.to_owned())),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::Google => "Google",
            Self::Discord => "Discord",
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::Google => "google",
            Self::Discord => "discord",
        }
    }

    fn env_prefix(self) -> &'static str {
        match self {
            Self::GitHub => "OAUTH_GITHUB",
            Self::Google => "OAUTH_GOOGLE",
            Self::Discord => "OAUTH_DISCORD",
        }
    }

    fn default_authorize_url(self) -> &'static str {
        match self {
            Self::GitHub => "https://github.com/login/oauth/authorize",
            Self::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            Self::Discord => "https://discord.com/oauth2/authorize",
        }
    }

    fn default_token_url(self) -> &'static str {
        match self {
            Self::GitHub => "https://github.com/login/oauth/access_token",
            Self::Google => "https://oauth2.googleapis.com/token",
            Self::Discord => "https://discord.com/api/oauth2/token",
        }
    }

    fn default_userinfo_url(self) -> &'static str {
        match self {
            Self::GitHub => "https://api.github.com/user",
            Self::Google => "https://openidconnect.googleapis.com/v1/userinfo",
            Self::Discord => "https://discord.com/api/users/@me",
        }
    }

    fn default_scopes(self) -> &'static str {
        match self {
            Self::GitHub => "read:user user:email",
            Self::Google => "openid profile email",
            Self::Discord => "identify email",
        }
    }

    fn default_redirect_url(self) -> String {
        let base_url =
            optional_env("XP_WIKI_BASE_URL").unwrap_or_else(|| "http://127.0.0.1:8080".to_owned());
        format!(
            "{}/auth/callback/{}",
            base_url.trim_end_matches('/'),
            self.slug()
        )
    }
}

fn load_env() {
    load_dotenv();
}

fn required_env(key: &str) -> Result<String, AuthError> {
    optional_env(key).ok_or_else(|| AuthError::MissingEnv(key.to_owned()))
}

fn post_login_redirect_url() -> String {
    let configured_url = optional_env(UI_URL_ENV);
    ui_redirect_url(configured_url.as_deref())
}

fn ui_redirect_url(configured_url: Option<&str>) -> String {
    configured_url
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .unwrap_or(DEFAULT_UI_REDIRECT_URL)
        .to_owned()
}

fn invalid_url(err: impl ToString) -> AuthError {
    AuthError::InvalidUrl(err.to_string())
}

fn session_id_from_headers(headers: &HeaderMap) -> Option<String> {
    cookie_value_from_headers(headers, SESSION_COOKIE)
}

fn take_pending_login(state: &str) -> Result<Option<PendingLogin>, AuthError> {
    Ok(PENDING_LOGINS
        .lock()
        .map_err(|_| AuthError::Lock)?
        .remove(state))
}

fn pending_login_cookie(pending_login: &PendingLogin) -> Result<String, AuthError> {
    let value = encode_pending_login(pending_login)?;
    Ok(format!(
        "{PENDING_LOGIN_COOKIE}={value}; Path=/auth/callback; HttpOnly; SameSite=Lax; Max-Age={PENDING_LOGIN_MAX_AGE_SECONDS}"
    ))
}

fn clear_pending_login_cookie() -> String {
    format!("{PENDING_LOGIN_COOKIE}=; Path=/auth/callback; HttpOnly; SameSite=Lax; Max-Age=0")
}

fn pending_login_from_headers(headers: &HeaderMap) -> Result<Option<PendingLogin>, AuthError> {
    cookie_value_from_headers(headers, PENDING_LOGIN_COOKIE)
        .map(|value| decode_pending_login(&value))
        .transpose()
}

fn encode_pending_login(pending_login: &PendingLogin) -> Result<String, AuthError> {
    let json = serde_json::to_vec(pending_login).map_err(|_| AuthError::InvalidStateCookie)?;
    Ok(URL_SAFE_NO_PAD.encode(json))
}

fn decode_pending_login(value: &str) -> Result<PendingLogin, AuthError> {
    let json = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AuthError::InvalidStateCookie)?;
    serde_json::from_slice(&json).map_err(|_| AuthError::InvalidStateCookie)
}

fn cookie_value_from_headers(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    let cookies = headers.get(COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|cookie| {
        let (name, value) = cookie.trim().split_once('=')?;
        if name == cookie_name && !value.is_empty() {
            Some(value.to_owned())
        } else {
            None
        }
    })
}

fn append_set_cookie(response: &mut Response, cookie: String) {
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(SET_COOKIE, value);
    }
}

fn redirect_response(location: &str) -> Response {
    let mut response = StatusCode::FOUND.into_response();
    if let Ok(location) = HeaderValue::from_str(location) {
        response.headers_mut().insert(LOCATION, location);
    }
    response
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (status, message.to_owned()).into_response()
}

fn json_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, fs};
    use tempfile::tempdir;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &std::path::Path) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);

            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    #[test]
    fn configured_providers_should_include_discord_from_configured_env_file() {
        let dir = tempdir().expect("temp dir should be created");
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            "OAUTH_DISCORD_CLIENT_ID=discord-client\nOAUTH_DISCORD_CLIENT_SECRET=discord-secret\n",
        )
        .expect("env file should be written");
        let _env_file = EnvVarGuard::set("XP_WIKI_ENV_FILE", &env_path);

        let providers = configured_providers();

        assert!(providers
            .iter()
            .any(|provider| provider.slug == "discord" && provider.label == "Discord"));
    }

    #[test]
    fn pending_login_from_headers_should_decode_cookie_fallback() {
        let pending_login = PendingLogin {
            provider: "discord".to_owned(),
            state: "state-1".to_owned(),
            pkce_verifier: "verifier-1".to_owned(),
        };
        let cookie = pending_login_cookie(&pending_login).expect("cookie should encode");
        let mut headers = HeaderMap::new();
        let cookie_pair = cookie
            .split_once(';')
            .map(|(pair, _)| pair)
            .expect("cookie should include attributes");
        headers.insert(
            COOKIE,
            HeaderValue::from_str(cookie_pair).expect("cookie value should be valid"),
        );

        let decoded = pending_login_from_headers(&headers)
            .expect("cookie should decode")
            .expect("cookie should exist");

        assert_eq!(decoded.provider, "discord");
        assert_eq!(decoded.state, "state-1");
        assert_eq!(decoded.pkce_verifier, "verifier-1");
    }

    #[test]
    fn ui_redirect_url_should_default_to_server_root_when_not_configured() {
        let url = ui_redirect_url(None);

        assert_eq!(url, "/");
    }

    #[test]
    fn ui_redirect_url_should_use_configured_ui_url() {
        let url = ui_redirect_url(Some(" http://127.0.0.1:3000/app "));

        assert_eq!(url, "http://127.0.0.1:3000/app");
    }
}
