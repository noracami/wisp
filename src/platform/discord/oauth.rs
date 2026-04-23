//! Discord OAuth2 `webhook.incoming` helpers for TPP POC Stage 2.
//!
//! Implements:
//! - stateless HMAC state token for CSRF protection
//! - authorize URL construction
//! - token exchange (code → webhook)

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

const STATE_TTL_SECS: u64 = 600;

pub const DISCORD_TOKEN_ENDPOINT: &str = "https://discord.com/api/oauth2/token";
const DISCORD_AUTHORIZE_ENDPOINT: &str = "https://discord.com/api/oauth2/authorize";

#[derive(Debug, Error)]
pub enum StateError {
    #[error("malformed state")]
    Malformed,
    #[error("invalid HMAC signature")]
    BadSignature,
    #[error("state expired")]
    Expired,
}

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("token exchange HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("token exchange returned {status}: {body}")]
    NonSuccess { status: u16, body: String },
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub token_type: String,
    pub access_token: String,
    pub scope: String,
    pub expires_in: i64,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub webhook: IncomingWebhook,
}

#[derive(Debug, Deserialize)]
pub struct IncomingWebhook {
    pub id: String,
    pub token: String,
    pub channel_id: String,
    #[serde(default)]
    pub guild_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    pub url: String,
}

pub fn generate_state(user_id: &str, secret: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let payload = format!("{user_id}|{ts}");

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(payload.as_bytes());
    let sig = mac.finalize().into_bytes();

    format!("{}.{}", B64.encode(payload), B64.encode(sig))
}

pub fn verify_state(state: &str, secret: &str) -> Result<String, StateError> {
    let (payload_b64, sig_b64) = state.split_once('.').ok_or(StateError::Malformed)?;
    let payload = B64.decode(payload_b64).map_err(|_| StateError::Malformed)?;
    let sig = B64.decode(sig_b64).map_err(|_| StateError::Malformed)?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(&payload);
    mac.verify_slice(&sig).map_err(|_| StateError::BadSignature)?;

    let payload_str = String::from_utf8(payload).map_err(|_| StateError::Malformed)?;
    let (user_id, ts_str) = payload_str.split_once('|').ok_or(StateError::Malformed)?;
    let ts: u64 = ts_str.parse().map_err(|_| StateError::Malformed)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now.saturating_sub(ts) > STATE_TTL_SECS {
        return Err(StateError::Expired);
    }

    Ok(user_id.to_string())
}

pub fn build_authorize_url(application_id: &str, redirect_uri: &str, state: &str) -> String {
    let mut url = url::Url::parse(DISCORD_AUTHORIZE_ENDPOINT).expect("valid base URL");
    url.query_pairs_mut()
        .append_pair("client_id", application_id)
        .append_pair("response_type", "code")
        .append_pair("scope", "webhook.incoming")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state);
    url.to_string()
}

pub async fn exchange_code(
    token_endpoint: &str,
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<TokenResponse, OAuthError> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
    ];
    let resp = reqwest::Client::new()
        .post(token_endpoint)
        .basic_auth(client_id, Some(client_secret))
        .form(&form)
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(OAuthError::NonSuccess {
            status: status.as_u16(),
            body,
        });
    }
    Ok(resp.json::<TokenResponse>().await?)
}
