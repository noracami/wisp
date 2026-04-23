use wisp::platform::discord::oauth::{
    OAuthError, StateError, build_authorize_url, exchange_code, generate_state, verify_state,
};

#[test]
fn state_roundtrip() {
    let token = generate_state("329579602429214721", "secret");
    let uid = verify_state(&token, "secret").unwrap();
    assert_eq!(uid, "329579602429214721");
}

#[test]
fn state_wrong_secret() {
    let token = generate_state("user", "secret");
    let err = verify_state(&token, "other-secret").unwrap_err();
    assert!(matches!(err, StateError::BadSignature));
}

#[test]
fn state_tampered_payload() {
    let token = generate_state("user", "secret");
    let (p, s) = token.split_once('.').unwrap();
    // Append a character to the base64 payload; will decode to different bytes
    // and either fail HMAC or fail payload parsing.
    let tampered = format!("{p}AA.{s}");
    let err = verify_state(&tampered, "secret").unwrap_err();
    assert!(matches!(
        err,
        StateError::BadSignature | StateError::Malformed
    ));
}

#[test]
fn state_malformed_no_dot() {
    let err = verify_state("not-a-state", "secret").unwrap_err();
    assert!(matches!(err, StateError::Malformed));
}

#[test]
fn state_malformed_bad_base64() {
    let err = verify_state("!!!.###", "secret").unwrap_err();
    assert!(matches!(err, StateError::Malformed));
}

#[test]
fn authorize_url_contains_expected_params() {
    let url = build_authorize_url("12345", "https://wisp.example.com/cb", "state123");
    assert!(url.starts_with("https://discord.com/api/oauth2/authorize?"));
    assert!(url.contains("client_id=12345"));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("scope=webhook.incoming"));
    assert!(url.contains("state=state123"));
    assert!(url.contains("redirect_uri=https%3A%2F%2Fwisp.example.com%2Fcb"));
}

#[tokio::test]
async fn exchange_code_happy_path() {
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token_type": "Bearer",
            "access_token": "at_xxx",
            "scope": "webhook.incoming",
            "expires_in": 604800,
            "refresh_token": "rt_xxx",
            "webhook": {
                "id": "wh123",
                "token": "wt456",
                "channel_id": "ch789",
                "guild_id": "g000",
                "name": "#general",
                "url": "https://discord.com/api/webhooks/wh123/wt456"
            }
        })))
        .mount(&server)
        .await;

    let endpoint = format!("{}/oauth2/token", server.uri());
    let r = exchange_code(&endpoint, "cid", "csecret", "code-xyz", "https://r")
        .await
        .unwrap();
    assert_eq!(r.webhook.id, "wh123");
    assert_eq!(r.webhook.token, "wt456");
    assert_eq!(r.webhook.channel_id, "ch789");
    assert_eq!(r.webhook.guild_id.as_deref(), Some("g000"));
    assert_eq!(r.webhook.name.as_deref(), Some("#general"));
}

#[tokio::test]
async fn exchange_code_non_success() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(400).set_body_string("invalid_grant"))
        .mount(&server)
        .await;

    let endpoint = format!("{}/oauth2/token", server.uri());
    let err = exchange_code(&endpoint, "cid", "csecret", "bad", "https://r")
        .await
        .unwrap_err();
    match err {
        OAuthError::NonSuccess { status, body } => {
            assert_eq!(status, 400);
            assert!(body.contains("invalid_grant"));
        }
        _ => panic!("expected NonSuccess"),
    }
}
