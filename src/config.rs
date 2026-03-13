use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub discord_application_id: String,
    pub discord_public_key: String,
    pub discord_bot_token: String,
    pub discord_webhook_url: String,
    pub anthropic_api_key: String,
    pub cwa_api_key: String,
    pub cwa_location: String,
    pub database_url: String,
    pub host: String,
    pub port: u16,
}

impl Config {
    pub fn new(
        discord_application_id: String,
        discord_public_key: String,
        discord_bot_token: String,
        discord_webhook_url: String,
        anthropic_api_key: String,
        cwa_api_key: String,
        cwa_location: String,
        database_url: String,
        host: String,
        port: u16,
    ) -> Self {
        Self {
            discord_application_id,
            discord_public_key,
            discord_bot_token,
            discord_webhook_url,
            anthropic_api_key,
            cwa_api_key,
            cwa_location,
            database_url,
            host,
            port,
        }
    }

    pub fn from_env() -> Result<Self, env::VarError> {
        Ok(Self::new(
            env::var("DISCORD_APPLICATION_ID")?,
            env::var("DISCORD_PUBLIC_KEY")?,
            env::var("DISCORD_BOT_TOKEN")?,
            env::var("DISCORD_WEBHOOK_URL")?,
            env::var("ANTHROPIC_API_KEY")?,
            env::var("CWA_API_KEY")?,
            env::var("CWA_LOCATION").unwrap_or_else(|_| "臺北市".to_string()),
            env::var("DATABASE_URL")?,
            env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("PORT must be a number"),
        ))
    }
}
