// tests/config_test.rs

#[test]
fn config_new_stores_all_fields() {
    let config = wisp::config::Config::new(
        "123".into(),
        "abc".into(),
        "token".into(),
        "https://discord.com/api/webhooks/test".into(),
        "sk-ant-test".into(),
        "cwa-key".into(),
        "臺北市".into(),
        "postgres://localhost/test".into(),
        "127.0.0.1".into(),
        3000,
    );

    assert_eq!(config.discord_application_id, "123");
    assert_eq!(config.discord_public_key, "abc");
    assert_eq!(config.host, "127.0.0.1");
    assert_eq!(config.port, 3000);
    assert_eq!(config.cwa_location, "臺北市");
}
