use ed25519_dalek::{SigningKey, Signer};

#[test]
fn verify_valid_signature_succeeds() {
    let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
    let public_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(public_key.as_bytes());

    let timestamp = "1234567890";
    let body = r#"{"type":1}"#;
    let message = format!("{timestamp}{body}");
    let signature = signing_key.sign(message.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());

    let result = wisp::platform::discord::verify::verify_signature(
        &public_key_hex,
        &signature_hex,
        timestamp,
        body.as_bytes(),
    );
    assert!(result.is_ok());
}

#[test]
fn verify_invalid_signature_fails() {
    let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
    let public_key = signing_key.verifying_key();
    let public_key_hex = hex::encode(public_key.as_bytes());

    let result = wisp::platform::discord::verify::verify_signature(
        &public_key_hex,
        &hex::encode([0u8; 64]),
        "1234567890",
        b"body",
    );
    assert!(result.is_err());
}
