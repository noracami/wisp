use axum::body::Bytes;
use axum::http::StatusCode;
use axum_test::TestServer;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;

fn create_test_app(signing_key: &SigningKey) -> TestServer {
    let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());
    let app = wisp::discord::interaction::test_router(public_key_hex);
    TestServer::new(app)
}

fn sign_request(signing_key: &SigningKey, timestamp: &str, body: &[u8]) -> String {
    let mut message = Vec::new();
    message.extend_from_slice(timestamp.as_bytes());
    message.extend_from_slice(body);
    hex::encode(signing_key.sign(&message).to_bytes())
}

#[tokio::test]
async fn ping_interaction_returns_pong() {
    let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
    let server = create_test_app(&signing_key);

    let body = json!({"type": 1}).to_string();
    let timestamp = "1234567890";
    let signature = sign_request(&signing_key, timestamp, body.as_bytes());

    let resp = server
        .post("/interactions")
        .add_header("X-Signature-Ed25519", signature.as_str())
        .add_header("X-Signature-Timestamp", timestamp)
        .content_type("application/json")
        .bytes(Bytes::from(body))
        .await;

    resp.assert_status(StatusCode::OK);
    let json: serde_json::Value = resp.json();
    assert_eq!(json["type"], 1); // PONG
}

#[tokio::test]
async fn invalid_signature_returns_401() {
    let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
    let server = create_test_app(&signing_key);

    let resp = server
        .post("/interactions")
        .add_header("X-Signature-Ed25519", hex::encode([0u8; 64]).as_str())
        .add_header("X-Signature-Timestamp", "123")
        .content_type("application/json")
        .bytes(Bytes::from(r#"{"type":1}"#))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);
}
