use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

fn sign_line_body(channel_secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(channel_secret.as_bytes()).unwrap();
    mac.update(body);
    BASE64.encode(mac.finalize().into_bytes())
}

#[test]
fn line_signature_valid() {
    let secret = "test-secret";
    let body = b"test body";
    let sig = sign_line_body(secret, body);

    assert!(wisp::platform::line::handler::verify_line_signature(secret, &sig, body).is_ok());
}

#[test]
fn line_signature_invalid() {
    let secret = "test-secret";
    let body = b"test body";

    assert!(wisp::platform::line::handler::verify_line_signature(secret, "invalid-sig", body).is_err());
}
