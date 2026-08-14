use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};

static ENV_LOADED: Lazy<()> = Lazy::new(|| {
    if let Some(path) = configured_env_file_path() {
        let _ = dotenvy::from_path_override(path);
        return;
    }

    let _ = dotenvy::dotenv_override();
});

pub fn load_dotenv() {
    Lazy::force(&ENV_LOADED);
}

pub fn optional_env(key: &str) -> Option<String> {
    optional_file_env(key).or_else(|| {
        load_dotenv();
        optional_process_env(key)
    })
}

fn optional_process_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn optional_file_env(key: &str) -> Option<String> {
    if let Some(path) = configured_env_file_path() {
        return value_from_env_file(&path, key);
    }

    dotenvy::dotenv_iter()
        .ok()?
        .filter_map(Result::ok)
        .find_map(|(name, value)| {
            if name == key && !value.trim().is_empty() {
                Some(value)
            } else {
                None
            }
        })
}

fn configured_env_file_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("XP_WIKI_ENV_FILE")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
    {
        return Some(path);
    }

    let manifest_env = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".env");
    manifest_env.exists().then_some(manifest_env)
}

fn value_from_env_file(path: &Path, key: &str) -> Option<String> {
    dotenvy::from_path_iter(path)
        .ok()?
        .filter_map(Result::ok)
        .find_map(|(name, value)| {
            if name == key && !value.trim().is_empty() {
                Some(value)
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn value_from_env_file_should_read_non_empty_provider_credentials() {
        let dir = tempdir().expect("temp dir should be created");
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            "OAUTH_DISCORD_CLIENT_ID=discord-client\nOAUTH_DISCORD_CLIENT_SECRET=discord-secret\n",
        )
        .expect("env file should be written");

        let value = value_from_env_file(&env_path, "OAUTH_DISCORD_CLIENT_ID");

        assert_eq!(value.as_deref(), Some("discord-client"));
    }
}
