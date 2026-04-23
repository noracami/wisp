use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub anthropic_api_key: String,
    pub database_url: String,
    pub cwa_api_key: String,
    pub cwa_location: String,
    pub host: String,
    pub port: u16,
    pub discord: Option<DiscordConfig>,
    pub line: Option<LineConfig>,
    pub google_search: Option<GoogleSearchConfig>,
}

#[derive(Debug, Clone)]
pub struct GoogleSearchConfig {
    pub api_key: String,
    pub engine_id: String,
}

#[derive(Debug, Clone)]
pub struct DiscordConfig {
    pub application_id: String,
    pub public_key: String,
    pub bot_token: String,
    pub webhook_url: String,
    pub client_secret: String,
    pub oauth_redirect_uri: String,
    pub state_secret: String,
}

#[derive(Debug, Clone)]
pub struct LineConfig {
    pub channel_secret: String,
    pub channel_access_token: String,
}

impl Config {
    pub fn from_env() -> Result<Self, env::VarError> {
        let discord = match (
            env::var("DISCORD_APPLICATION_ID"),
            env::var("DISCORD_PUBLIC_KEY"),
            env::var("DISCORD_BOT_TOKEN"),
            env::var("DISCORD_WEBHOOK_URL"),
            env::var("DISCORD_CLIENT_SECRET"),
            env::var("DISCORD_OAUTH_REDIRECT_URI"),
            env::var("TPP_STATE_SECRET"),
        ) {
            (
                Ok(app_id),
                Ok(pub_key),
                Ok(bot_token),
                Ok(webhook_url),
                Ok(client_secret),
                Ok(oauth_redirect_uri),
                Ok(state_secret),
            ) => Some(DiscordConfig {
                application_id: app_id,
                public_key: pub_key,
                bot_token,
                webhook_url,
                client_secret,
                oauth_redirect_uri,
                state_secret,
            }),
            _ => None,
        };

        let line = match (
            env::var("LINE_CHANNEL_SECRET"),
            env::var("LINE_CHANNEL_ACCESS_TOKEN"),
        ) {
            (Ok(secret), Ok(token)) => Some(LineConfig {
                channel_secret: secret,
                channel_access_token: token,
            }),
            _ => None,
        };

        let google_search = match (
            env::var("GOOGLE_SEARCH_API_KEY"),
            env::var("GOOGLE_SEARCH_ENGINE_ID"),
        ) {
            (Ok(api_key), Ok(engine_id)) => Some(GoogleSearchConfig {
                api_key,
                engine_id,
            }),
            _ => None,
        };

        Ok(Self {
            anthropic_api_key: env::var("ANTHROPIC_API_KEY")?,
            database_url: env::var("DATABASE_URL")?,
            cwa_api_key: env::var("CWA_API_KEY")?,
            cwa_location: env::var("CWA_LOCATION").unwrap_or_else(|_| "臺北市".to_string()),
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("PORT must be a number"),
            discord,
            line,
            google_search,
        })
    }
}
