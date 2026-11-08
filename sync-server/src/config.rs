use std::env;

/// The process's settings, all from the environment, as Cloud Run provides them.
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    /// The Supabase project the apps sign in with, `https://<ref>.supabase.co`.
    /// Its published keys check every access token.
    pub supabase_url: String,
    /// The project's legacy HS256 JWT secret, for a project that still signs
    /// with it. New projects sign with an asymmetric key and need none.
    pub supabase_jwt_secret: Option<String>,
    /// The project's secret API key, needed only to delete accounts.
    pub supabase_secret_key: Option<String>,
}

fn non_empty(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0} is not set")]
    Missing(&'static str),
    #[error("PORT is not a port number: {0}")]
    BadPort(String),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = non_empty("DATABASE_URL").ok_or(ConfigError::Missing("DATABASE_URL"))?;
        let supabase_url = non_empty("SUPABASE_URL").ok_or(ConfigError::Missing("SUPABASE_URL"))?;
        let port = match env::var("PORT") {
            Ok(value) => value
                .trim()
                .parse()
                .map_err(|_| ConfigError::BadPort(value))?,
            Err(_) => 8080,
        };
        Ok(Config {
            database_url,
            port,
            supabase_url,
            supabase_jwt_secret: non_empty("SUPABASE_JWT_SECRET"),
            supabase_secret_key: non_empty("SUPABASE_SECRET_KEY"),
        })
    }
}
