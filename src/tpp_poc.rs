//! Webhook-Interaction Bridge POC — Stage 1.
//!
//! Empirically validates whether manually-created Discord webhooks can deliver
//! button-click interactions to this app's Interactions Endpoint, combined with
//! user-installed slash commands. See
//! `docs/superpowers/specs/2026-04-23-webhook-interaction-bridge-poc-stage1-design.md`.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory registry: one webhook URL per user (the user who ran `/tpp-setup`).
pub struct PocState {
    pub webhooks: RwLock<HashMap<String, String>>,
}

impl PocState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            webhooks: RwLock::new(HashMap::new()),
        })
    }
}

impl Default for PocState {
    fn default() -> Self {
        Self {
            webhooks: RwLock::new(HashMap::new()),
        }
    }
}
