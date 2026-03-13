use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::error::AppError;

pub fn verify_signature(
    public_key_hex: &str,
    signature_hex: &str,
    timestamp: &str,
    body: &[u8],
) -> Result<(), AppError> {
    let pub_key_bytes = hex::decode(public_key_hex).map_err(|_| AppError::VerificationFailed)?;
    let sig_bytes = hex::decode(signature_hex).map_err(|_| AppError::VerificationFailed)?;

    let public_key = VerifyingKey::from_bytes(
        pub_key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| AppError::VerificationFailed)?,
    )
    .map_err(|_| AppError::VerificationFailed)?;

    let signature = Signature::from_bytes(
        sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| AppError::VerificationFailed)?,
    );

    let mut message = Vec::with_capacity(timestamp.len() + body.len());
    message.extend_from_slice(timestamp.as_bytes());
    message.extend_from_slice(body);

    public_key
        .verify(&message, &signature)
        .map_err(|_| AppError::VerificationFailed)
}
