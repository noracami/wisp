use wisp::config::{Config, DiscordConfig, LineConfig};

#[test]
fn config_with_discord_only() {
    let config = Config {
        anthropic_api_key: "test-key".to_string(),
        database_url: "postgres://localhost/test".to_string(),
        cwa_api_key: "test-cwa".to_string(),
        cwa_location: "臺北市".to_string(),
        host: "0.0.0.0".to_string(),
        port: 8080,
        discord: Some(DiscordConfig {
            application_id: "12345".to_string(),
            public_key: "abcdef".to_string(),
            bot_token: "bot-token".to_string(),
            webhook_url: "https://discord.com/webhook".to_string(),
        }),
        line: None,
        google_search: None,
    };
    assert!(config.discord.is_some());
    assert!(config.line.is_none());
}

#[test]
fn config_with_both_platforms() {
    let config = Config {
        anthropic_api_key: "test-key".to_string(),
        database_url: "postgres://localhost/test".to_string(),
        cwa_api_key: "test-cwa".to_string(),
        cwa_location: "臺北市".to_string(),
        host: "0.0.0.0".to_string(),
        port: 8080,
        discord: Some(DiscordConfig {
            application_id: "12345".to_string(),
            public_key: "abcdef".to_string(),
            bot_token: "bot-token".to_string(),
            webhook_url: "https://discord.com/webhook".to_string(),
        }),
        line: Some(LineConfig {
            channel_secret: "line-secret".to_string(),
            channel_access_token: "line-token".to_string(),
        }),
    };
        google_search: None,
    };
    assert!(config.discord.is_some());
    assert!(config.line.is_some());
}
