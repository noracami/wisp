# Multi-Platform Assistant Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor Wisp from a Discord-only bot into a platform-agnostic AI assistant service with unified user identity, layered architecture (Platform → Core → Tool), and LINE Bot support.

**Architecture:** Three-layer design — Platform Layer handles protocol-specific concerns (signature verification, message parsing, response delivery), Core Layer manages conversations and LLM interaction (completely platform-unaware), Tool Layer provides extensible capabilities via LLM function calling. Unified user identity with platform_identities mapping table.

**Tech Stack:** Rust (2024 edition), Axum 0.8, SQLx + PostgreSQL/pgvector, Anthropic Claude API, twilight-http (Discord), LINE Messaging API, HMAC-SHA256 (LINE verification), ed25519-dalek (Discord verification), wiremock (testing), axum-test (testing)

**Spec:** `docs/superpowers/specs/2026-03-19-multi-platform-assistant-design.md`

---

## File Map

### New files to create

| File | Responsibility |
|------|---------------|
| `src/platform/mod.rs` | `Platform` enum, `ChatRequest`, `ChatResponse`, `ChatMessage` types |
| `src/platform/discord/mod.rs` | Re-exports for discord platform module |
| `src/platform/discord/handler.rs` | Discord interaction endpoint, deferred response pattern |
| `src/platform/discord/verify.rs` | Ed25519 signature verification (moved from `src/discord/verify.rs`) |
| `src/platform/discord/webhook.rs` | Discord webhook client (moved from `src/discord/webhook.rs`) |
| `src/platform/line/mod.rs` | Re-exports for line platform module |
| `src/platform/line/handler.rs` | LINE webhook handler + HMAC-SHA256 verification |
| `src/platform/line/client.rs` | LINE Messaging API client (reply + push) |
| `src/assistant/mod.rs` | Re-exports for assistant module |
| `src/assistant/service.rs` | Central assistant: ChatRequest → memory → LLM → tool loop → ChatResponse |
| `src/db/users.rs` | UserService: resolve/create users + platform identities |
| `src/tools/mod.rs` | `Tool` trait, `ToolRegistry` |
| `src/tools/weather.rs` | `WeatherTool` implementing `Tool` trait |
| `migrations/002_multi_platform.sql` | New schema: users, platform_identities, updated conversations |
| `tests/platform_types_test.rs` | Tests for Platform enum serialization and ChatRequest/ChatResponse |
| `tests/db_users_test.rs` | Tests for UserService |
| `tests/assistant_test.rs` | Tests for Assistant handle flow |
| `tests/tools_registry_test.rs` | Tests for ToolRegistry and WeatherTool |
| `tests/line_handler_test.rs` | Tests for LINE webhook handler + signature verification |
| `tests/line_client_test.rs` | Tests for LINE Messaging API client |

### Files to modify

| File | Change |
|------|--------|
| `src/lib.rs` | Add `platform`, `assistant`, `tools` modules; keep `discord` until Task 7 removes it |
| `src/config.rs` | Make Discord config optional, add LINE config fields |
| `src/error.rs` | Add LINE-related error variants |
| `src/llm/mod.rs` | Remove duplicate `ChatMessage`, re-export from `platform` |
| `src/llm/claude.rs` | Add `tools` parameter, parse `tool_use` content blocks, return structured response |
| `src/db/mod.rs` | Add `users` module, update migration runner |
| `src/db/memory.rs` | Replace `discord_user_id`/`discord_channel_id` with `user_id` UUID, fix message ordering |
| `src/main.rs` | New startup flow: build Assistant, nest platform routes, optional platform loading |
| `src/scheduler.rs` | Update imports for moved webhook module |
| `tests/discord_verify_test.rs` | Update imports from `discord::verify` to `platform::discord::verify` |
| `tests/discord_webhook_test.rs` | Update imports |
| `tests/interaction_test.rs` | Update imports, adapt to new handler structure |
| `tests/db_memory_test.rs` | Update to use `user_id: Uuid` instead of `discord_user_id: String` |
| `tests/config_test.rs` | Update for optional platform config |

### Files to delete

| File | Reason |
|------|--------|
| `src/discord/mod.rs` | Replaced by `src/platform/discord/mod.rs` |
| `src/discord/interaction.rs` | Replaced by `src/platform/discord/handler.rs` + `src/core/assistant.rs` |
| `src/discord/verify.rs` | Moved to `src/platform/discord/verify.rs` |
| `src/discord/webhook.rs` | Moved to `src/platform/discord/webhook.rs` |

---

## Task 1: Database schema — users and platform identities

**Files:**
- Create: `migrations/002_multi_platform.sql`
- Modify: `src/db/mod.rs`
- Create: `src/db/users.rs`
- Create: `tests/db_users_test.rs`

- [ ] **Step 1: Write the migration SQL**

Create `migrations/002_multi_platform.sql`:

```sql
-- Drop old tables (early stage, data can be discarded)
DROP TABLE IF EXISTS messages CASCADE;
DROP TABLE IF EXISTS conversations CASCADE;

-- Unified users
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    display_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Platform identity mapping
CREATE TABLE platform_identities (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id),
    platform TEXT NOT NULL,
    platform_user_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (platform, platform_user_id)
);

-- Conversations bound to unified user_id
CREATE TABLE conversations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id),
    channel_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_conversations_user_channel
    ON conversations(user_id, channel_id, platform, updated_at DESC);

-- Messages (recreated)
CREATE TABLE messages (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    conversation_id UUID NOT NULL REFERENCES conversations(id),
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content TEXT NOT NULL,
    embedding VECTOR(1024),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_messages_conversation ON messages(conversation_id, created_at);
CREATE INDEX idx_messages_embedding ON messages
    USING hnsw (embedding vector_cosine_ops);
```

- [ ] **Step 2: Update migration runner in `src/db/mod.rs`**

Add the new migration file to `run_migrations`:

```rust
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(include_str!("../../migrations/001_init.sql"))
        .execute(pool)
        .await?;
    sqlx::raw_sql(include_str!("../../migrations/002_multi_platform.sql"))
        .execute(pool)
        .await?;
    Ok(())
}
```

- [ ] **Step 3: Write failing tests for UserService**

Create `tests/db_users_test.rs`:

```rust
use sqlx::PgPool;
use wisp::db::{create_pool, run_migrations};
use wisp::db::users::UserService;

async fn setup_db() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string());
    let pool = create_pool(&url).await.expect("Failed to connect to DB");
    run_migrations(&pool).await.expect("Failed to run migrations");
    pool
}

#[tokio::test]
#[ignore] // Requires database
async fn resolve_or_create_creates_new_user() {
    let pool = setup_db().await;
    let svc = UserService::new(pool);

    let user_id = svc.resolve_or_create("discord", "123456").await.unwrap();
    assert!(!user_id.is_nil());

    // Same platform + platform_user_id should return same user
    let user_id_again = svc.resolve_or_create("discord", "123456").await.unwrap();
    assert_eq!(user_id, user_id_again);
}

#[tokio::test]
#[ignore] // Requires database
async fn different_platforms_create_different_users() {
    let pool = setup_db().await;
    let svc = UserService::new(pool);

    let discord_user = svc.resolve_or_create("discord", "user_abc").await.unwrap();
    let line_user = svc.resolve_or_create("line", "user_abc").await.unwrap();

    // Same platform_user_id but different platforms → different users
    assert_ne!(discord_user, line_user);
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test --test db_users_test -- --ignored 2>&1 | tail -5`
Expected: Compilation error — `wisp::db::users` does not exist

- [ ] **Step 5: Implement UserService**

Create `src/db/users.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

pub struct UserService {
    pool: PgPool,
}

impl UserService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Resolve a platform identity to a unified user ID.
    /// Creates a new user + identity if not found.
    pub async fn resolve_or_create(
        &self,
        platform: &str,
        platform_user_id: &str,
    ) -> Result<Uuid, sqlx::Error> {
        // Try to find existing identity
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT user_id FROM platform_identities
             WHERE platform = $1 AND platform_user_id = $2",
        )
        .bind(platform)
        .bind(platform_user_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((user_id,)) = existing {
            return Ok(user_id);
        }

        // Create new user + identity in a transaction
        let mut tx = self.pool.begin().await?;

        let (user_id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO users DEFAULT VALUES RETURNING id",
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO platform_identities (user_id, platform, platform_user_id)
             VALUES ($1, $2, $3)",
        )
        .bind(user_id)
        .bind(platform)
        .bind(platform_user_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(user_id)
    }
}
```

- [ ] **Step 6: Register module in `src/db/mod.rs`**

Add `pub mod users;` to `src/db/mod.rs`.

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test --test db_users_test -- --ignored 2>&1 | tail -5`
Expected: 2 tests PASS

- [ ] **Step 8: Commit**

```bash
git add migrations/002_multi_platform.sql src/db/users.rs src/db/mod.rs tests/db_users_test.rs
git commit -m "feat: add users + platform_identities schema and UserService"
```

---

## Task 2: Platform types and shared types

**Files:**
- Create: `src/platform/mod.rs`
- Create: `src/assistant/mod.rs` (stub)
- Create: `src/assistant/service.rs` (stub)
- Create: `tests/platform_types_test.rs`
- Modify: `src/lib.rs`
- Modify: `src/llm/mod.rs`

> **Note:** `src/lib.rs` keeps `pub mod discord;` until Task 7 to avoid breaking existing tests.

- [ ] **Step 1: Write failing tests for platform types**

Create `tests/platform_types_test.rs`:

```rust
use wisp::platform::{Platform, ChatRequest, ChatResponse, ChatMessage};
use uuid::Uuid;

#[test]
fn platform_to_db_string() {
    assert_eq!(Platform::Discord.as_str(), "discord");
    assert_eq!(Platform::Line.as_str(), "line");
}

#[test]
fn platform_from_db_string() {
    assert_eq!(Platform::from_str("discord"), Some(Platform::Discord));
    assert_eq!(Platform::from_str("line"), Some(Platform::Line));
    assert_eq!(Platform::from_str("unknown"), None);
}

#[test]
fn chat_request_construction() {
    let req = ChatRequest {
        user_id: Uuid::new_v4(),
        channel_id: "ch-123".to_string(),
        platform: Platform::Discord,
        message: "hello".to_string(),
    };
    assert_eq!(req.message, "hello");
    assert_eq!(req.platform.as_str(), "discord");
}

#[test]
fn chat_response_construction() {
    let resp = ChatResponse {
        text: "hi there".to_string(),
    };
    assert_eq!(resp.text, "hi there");
}

#[test]
fn chat_message_construction() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "hello".to_string(),
    };
    assert_eq!(msg.role, "user");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test platform_types_test 2>&1 | tail -5`
Expected: Compilation error — `wisp::platform` does not exist

- [ ] **Step 3: Implement platform types**

Create `src/platform/mod.rs`:

```rust
pub mod discord;
pub mod line;

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Discord,
    Line,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Discord => "discord",
            Platform::Line => "line",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "discord" => Some(Platform::Discord),
            "line" => Some(Platform::Line),
            _ => None,
        }
    }
}

/// Shared message type used across LLM, memory, and assistant layers.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Unified request from Platform Layer to Assistant
pub struct ChatRequest {
    pub user_id: Uuid,
    pub channel_id: String,
    pub platform: Platform,
    pub message: String,
}

/// Assistant response back to Platform Layer
pub struct ChatResponse {
    pub text: String,
}
```

Note: `pub mod discord;` and `pub mod line;` will cause compilation errors until those modules exist. Create stub modules:

Create `src/platform/discord/mod.rs`:
```rust
pub mod handler;
pub mod verify;
pub mod webhook;
```

Create `src/platform/line/mod.rs`:
```rust
pub mod handler;
pub mod client;
```

Create stub files for each submodule (empty files):
- `src/platform/discord/handler.rs`
- `src/platform/discord/verify.rs`
- `src/platform/discord/webhook.rs`
- `src/platform/line/handler.rs`
- `src/platform/line/client.rs`

- [ ] **Step 4: Create assistant module stubs**

Create `src/assistant/mod.rs`:
```rust
pub mod service;
```

Create `src/assistant/service.rs` (stub):
```rust
// Will be implemented in Task 6
```

- [ ] **Step 5: Update lib.rs and llm re-exports**

Update `src/llm/mod.rs` to re-export `ChatMessage` from platform:

```rust
pub mod claude;

pub use crate::platform::ChatMessage;
```

Update `src/lib.rs` — add new modules, **keep `pub mod discord;`** for now:

```rust
pub mod assistant;
pub mod config;
pub mod db;
pub mod discord;  // kept until Task 7
pub mod error;
pub mod llm;
pub mod platform;
pub mod tools;
pub mod weather;
```

Note: `pub mod tools;` will need a stub. Create `src/tools/mod.rs`:
```rust
// Will be implemented in Task 4
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --test platform_types_test 2>&1 | tail -5`
Expected: 5 tests PASS

Also verify existing tests still pass:
Run: `cargo test 2>&1 | tail -5`
Expected: All non-ignored tests PASS (old discord tests still work)

- [ ] **Step 7: Commit**

```bash
git add src/platform/ src/assistant/ src/tools/mod.rs src/lib.rs src/llm/mod.rs tests/platform_types_test.rs
git commit -m "feat: add Platform types, ChatRequest/ChatResponse, unify ChatMessage"
```

---

## Task 3: Update Memory to use unified user_id

**Files:**
- Modify: `src/db/memory.rs`
- Modify: `tests/db_memory_test.rs`

- [ ] **Step 1: Update failing tests for Memory with Uuid user_id**

Update `tests/db_memory_test.rs` — replace `discord_user_id: &str` / `discord_channel_id: &str` with `user_id: Uuid` / `channel_id: &str` / `platform: &str`. Also test the fixed message ordering (most recent N, ASC order):

```rust
use sqlx::PgPool;
use uuid::Uuid;
use wisp::db::{create_pool, run_migrations};
use wisp::db::memory::Memory;

async fn setup_db() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string());
    let pool = create_pool(&url).await.expect("Failed to connect to DB");
    run_migrations(&pool).await.expect("Failed to run migrations");
    pool
}

#[tokio::test]
#[ignore]
async fn conversation_lifecycle() {
    let pool = setup_db().await;
    let memory = Memory::new(pool);
    let user_id = Uuid::new_v4();

    let conv_id = memory
        .get_or_create_conversation(user_id, "channel-1", "discord")
        .await
        .unwrap();
    assert!(!conv_id.is_nil());

    // Same user/channel/platform within 30 min → same conversation
    let conv_id_again = memory
        .get_or_create_conversation(user_id, "channel-1", "discord")
        .await
        .unwrap();
    assert_eq!(conv_id, conv_id_again);
}

#[tokio::test]
#[ignore]
async fn load_recent_messages_returns_most_recent_in_order() {
    let pool = setup_db().await;
    let memory = Memory::new(pool);
    let user_id = Uuid::new_v4();

    let conv_id = memory
        .get_or_create_conversation(user_id, "channel-order", "discord")
        .await
        .unwrap();

    // Store 5 messages
    for i in 1..=5 {
        memory
            .store_message(conv_id, "user", &format!("msg-{i}"), None)
            .await
            .unwrap();
    }

    // Load last 3 — should be msg-3, msg-4, msg-5 in ASC order
    let messages = memory.load_recent_messages(conv_id, 3).await.unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].content, "msg-3");
    assert_eq!(messages[1].content, "msg-4");
    assert_eq!(messages[2].content, "msg-5");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test db_memory_test -- --ignored 2>&1 | tail -5`
Expected: Compilation error — `get_or_create_conversation` signature mismatch

- [ ] **Step 3: Update Memory implementation**

Update `src/db/memory.rs`:

```rust
use pgvector::Vector;
use sqlx::PgPool;
use uuid::Uuid;

use crate::platform::ChatMessage;

pub struct Memory {
    pool: PgPool,
}

impl Memory {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_or_create_conversation(
        &self,
        user_id: Uuid,
        channel_id: &str,
        platform: &str,
    ) -> Result<Uuid, sqlx::Error> {
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM conversations
             WHERE user_id = $1 AND channel_id = $2 AND platform = $3
             AND updated_at > now() - interval '30 minutes'
             ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(channel_id)
        .bind(platform)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((id,)) = existing {
            // Refresh updated_at
            sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await?;
            return Ok(id);
        }

        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO conversations (user_id, channel_id, platform)
             VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(user_id)
        .bind(channel_id)
        .bind(platform)
        .fetch_one(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn store_message(
        &self,
        conversation_id: Uuid,
        role: &str,
        content: &str,
        embedding: Option<Vec<f32>>,
    ) -> Result<(), sqlx::Error> {
        let emb = embedding.map(Vector::from);
        sqlx::query(
            "INSERT INTO messages (conversation_id, role, content, embedding)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(conversation_id)
        .bind(role)
        .bind(content)
        .bind(emb)
        .execute(&self.pool)
        .await?;

        // Update conversation's updated_at
        sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
            .bind(conversation_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn load_recent_messages(
        &self,
        conversation_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ChatMessage>, sqlx::Error> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT role, content FROM (
                SELECT role, content, created_at FROM messages
                WHERE conversation_id = $1
                ORDER BY created_at DESC
                LIMIT $2
             ) sub ORDER BY created_at ASC",
        )
        .bind(conversation_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(role, content)| ChatMessage { role, content })
            .collect())
    }

    pub async fn search_similar(
        &self,
        user_id: Uuid,
        query_embedding: Vec<f32>,
        limit: i64,
    ) -> Result<Vec<ChatMessage>, sqlx::Error> {
        let emb = Vector::from(query_embedding);
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT m.role, m.content
             FROM messages m
             JOIN conversations c ON m.conversation_id = c.id
             WHERE c.user_id = $1 AND m.embedding IS NOT NULL
             ORDER BY m.embedding <=> $2
             LIMIT $3",
        )
        .bind(user_id)
        .bind(emb)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(role, content)| ChatMessage { role, content })
            .collect())
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test db_memory_test -- --ignored 2>&1 | tail -5`
Expected: 2 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/db/memory.rs tests/db_memory_test.rs
git commit -m "feat: update Memory to use unified user_id, fix message ordering"
```

---

## Task 4: Tool trait and ToolRegistry

**Files:**
- Create: `src/tools/mod.rs` (replace stub)
- Create: `src/tools/weather.rs`
- Create: `tests/tools_registry_test.rs`

- [ ] **Step 1: Write failing tests for ToolRegistry**

Create `tests/tools_registry_test.rs`:

```rust
use serde_json::{json, Value};
use wisp::tools::{ToolRegistry, Tool};
use wisp::error::AppError;
use async_trait::async_trait;

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "Echoes input back" }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string" }
            },
            "required": ["text"]
        })
    }
    async fn execute(&self, input: Value) -> Result<String, AppError> {
        Ok(input["text"].as_str().unwrap_or("").to_string())
    }
}

#[test]
fn registry_tool_definitions() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(EchoTool));

    let defs = registry.tool_definitions();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0]["name"], "echo");
    assert_eq!(defs[0]["description"], "Echoes input back");
}

#[tokio::test]
async fn registry_execute_known_tool() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(EchoTool));

    let result = registry.execute("echo", json!({"text": "hello"})).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "hello");
}

#[tokio::test]
async fn registry_execute_unknown_tool() {
    let registry = ToolRegistry::new();
    let result = registry.execute("nonexistent", json!({})).await;
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test tools_registry_test 2>&1 | tail -5`
Expected: Compilation error — `wisp::tools::ToolRegistry` does not exist

- [ ] **Step 3: Add async-trait dependency**

Add to `Cargo.toml` under `[dependencies]`:

```toml
async-trait = "0.1"
```

- [ ] **Step 4: Implement Tool trait and ToolRegistry**

Replace `src/tools/mod.rs`:

```rust
pub mod weather;

use std::collections::HashMap;
use async_trait::async_trait;
use serde_json::Value;

use crate::error::AppError;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn execute(&self, input: Value) -> Result<String, AppError>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn tool_definitions(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|t| {
                serde_json::json!({
                    "name": t.name(),
                    "description": t.description(),
                    "input_schema": t.parameters(),
                })
            })
            .collect()
    }

    pub async fn execute(&self, name: &str, input: Value) -> Result<String, AppError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| AppError::Internal(format!("Unknown tool: {name}")))?;
        tool.execute(input).await
    }
}
```

- [ ] **Step 5: Implement WeatherTool**

Create `src/tools/weather.rs`:

```rust
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::weather::cwa::CwaClient;
use super::Tool;

pub struct WeatherTool {
    cwa_client: CwaClient,
}

impl WeatherTool {
    pub fn new(cwa_client: CwaClient) -> Self {
        Self { cwa_client }
    }
}

#[async_trait]
impl Tool for WeatherTool {
    fn name(&self) -> &str {
        "get_weather"
    }

    fn description(&self) -> &str {
        "取得台灣指定地區的天氣預報"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "地區名稱，例如：臺北市"
                }
            },
            "required": ["location"]
        })
    }

    async fn execute(&self, input: Value) -> Result<String, AppError> {
        let location = input["location"].as_str().unwrap_or("臺北市");
        let forecast = self
            .cwa_client
            .fetch_forecast(location)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(forecast.to_embed_description())
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --test tools_registry_test 2>&1 | tail -5`
Expected: 3 tests PASS

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/tools/ tests/tools_registry_test.rs
git commit -m "feat: add Tool trait, ToolRegistry, and WeatherTool"
```

---

## Task 5: Claude client — tool use support

**Files:**
- Modify: `src/llm/claude.rs`
- Modify: `tests/llm_claude_test.rs`

- [ ] **Step 1: Write failing tests for tool use response parsing**

Add to `tests/llm_claude_test.rs`:

```rust
#[tokio::test]
async fn chat_with_tools_returns_tool_use() {
    let mock_server = wiremock::MockServer::start().await;

    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/messages"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [
                {
                    "type": "tool_use",
                    "id": "toolu_123",
                    "name": "get_weather",
                    "input": {"location": "臺北市"}
                }
            ],
            "stop_reason": "tool_use"
        })))
        .mount(&mock_server)
        .await;

    let client = wisp::llm::claude::ClaudeClient::new("test-key", &mock_server.uri());
    let tools = vec![serde_json::json!({
        "name": "get_weather",
        "description": "Get weather forecast",
        "input_schema": {
            "type": "object",
            "properties": {
                "location": {"type": "string"}
            }
        }
    })];
    let messages = vec![wisp::platform::ChatMessage {
        role: "user".to_string(),
        content: "What's the weather in Taipei?".to_string(),
    }];

    let response = client.chat(&messages, None, Some(&tools)).await.unwrap();

    match response {
        wisp::llm::claude::LlmResponse::Text(t) => panic!("Expected ToolUse, got Text: {t}"),
        wisp::llm::claude::LlmResponse::ToolUse { id, name, input } => {
            assert_eq!(name, "get_weather");
            assert_eq!(id, "toolu_123");
            assert_eq!(input["location"], "臺北市");
        }
    }
}

#[tokio::test]
async fn chat_with_tools_returns_text() {
    let mock_server = wiremock::MockServer::start().await;

    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/messages"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "The weather is sunny."}],
            "stop_reason": "end_turn"
        })))
        .mount(&mock_server)
        .await;

    let client = wisp::llm::claude::ClaudeClient::new("test-key", &mock_server.uri());
    let messages = vec![wisp::platform::ChatMessage {
        role: "user".to_string(),
        content: "hello".to_string(),
    }];

    let response = client.chat(&messages, None, Some(&vec![])).await.unwrap();

    match response {
        wisp::llm::claude::LlmResponse::Text(t) => assert_eq!(t, "The weather is sunny."),
        wisp::llm::claude::LlmResponse::ToolUse { .. } => panic!("Expected Text, got ToolUse"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test llm_claude_test 2>&1 | tail -5`
Expected: Compilation error — `LlmResponse` does not exist, `chat` signature mismatch

- [ ] **Step 3: Update ClaudeClient to support tools and return LlmResponse**

Update `src/llm/claude.rs`:

```rust
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::platform::ChatMessage;
use crate::error::AppError;

pub struct ClaudeClient {
    api_key: String,
    base_url: String,
    http: Client,
}

#[derive(Debug)]
pub enum LlmResponse {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<ClaudeMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
}

#[derive(Serialize)]
struct ClaudeMessage {
    role: String,
    content: ClaudeContent,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ClaudeContent {
    Text(String),
    Blocks(Vec<Value>),
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

impl ClaudeClient {
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: base_url.to_string(),
            http: Client::new(),
        }
    }

    pub fn with_default_url(api_key: &str) -> Self {
        Self::new(api_key, "https://api.anthropic.com")
    }

    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        system_prompt: Option<&str>,
        tools: Option<&Vec<Value>>,
    ) -> Result<LlmResponse, AppError> {
        let request = ClaudeRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 1024,
            system: system_prompt.map(|s| s.to_string()),
            messages: messages
                .iter()
                .map(|m| ClaudeMessage {
                    role: m.role.clone(),
                    content: ClaudeContent::Text(m.content.clone()),
                })
                .collect(),
            tools: tools.cloned(),
        };

        let resp: ClaudeResponse = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::Request)?
            .json()
            .await?;

        // Check for tool_use first
        for block in &resp.content {
            if let ContentBlock::ToolUse { id, name, input } = block {
                return Ok(LlmResponse::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
            }
        }

        // Fall back to text
        for block in resp.content {
            if let ContentBlock::Text { text } = block {
                return Ok(LlmResponse::Text(text));
            }
        }

        Err(AppError::Internal("Empty response from Claude".to_string()))
    }

    /// Send tool result back to Claude for continued processing.
    pub async fn chat_with_tool_result(
        &self,
        messages: &[ChatMessage],
        tool_use_id: &str,
        tool_use_name: &str,
        tool_use_input: &Value,
        tool_result: &str,
        system_prompt: Option<&str>,
        tools: Option<&Vec<Value>>,
    ) -> Result<LlmResponse, AppError> {
        let mut claude_messages: Vec<ClaudeMessage> = messages
            .iter()
            .map(|m| ClaudeMessage {
                role: m.role.clone(),
                content: ClaudeContent::Text(m.content.clone()),
            })
            .collect();

        // Add assistant's tool_use message
        claude_messages.push(ClaudeMessage {
            role: "assistant".to_string(),
            content: ClaudeContent::Blocks(vec![serde_json::json!({
                "type": "tool_use",
                "id": tool_use_id,
                "name": tool_use_name,
                "input": tool_use_input,
            })]),
        });

        // Add user's tool_result message
        claude_messages.push(ClaudeMessage {
            role: "user".to_string(),
            content: ClaudeContent::Blocks(vec![serde_json::json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": tool_result,
            })]),
        });

        let request = ClaudeRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 1024,
            system: system_prompt.map(|s| s.to_string()),
            messages: claude_messages,
            tools: tools.cloned(),
        };

        let resp: ClaudeResponse = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::Request)?
            .json()
            .await?;

        for block in &resp.content {
            if let ContentBlock::ToolUse { id, name, input } = block {
                return Ok(LlmResponse::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
            }
        }

        for block in resp.content {
            if let ContentBlock::Text { text } = block {
                return Ok(LlmResponse::Text(text));
            }
        }

        Err(AppError::Internal("Empty response from Claude".to_string()))
    }
}
```

- [ ] **Step 4: Update existing claude test for new signature**

The existing `chat_returns_text_response` test in `tests/llm_claude_test.rs` needs to be updated for the new `chat` signature (add `None` for tools parameter) and unwrap `LlmResponse::Text`:

```rust
// Update the existing test's chat call:
let response = client.chat(&messages, None, None).await.unwrap();
match response {
    wisp::llm::claude::LlmResponse::Text(t) => assert_eq!(t, "Hello! How can I help?"),
    _ => panic!("Expected Text response"),
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test llm_claude_test 2>&1 | tail -5`
Expected: 3 tests PASS (1 existing + 2 new)

- [ ] **Step 6: Commit**

```bash
git add src/llm/claude.rs tests/llm_claude_test.rs
git commit -m "feat: add tool use support to ClaudeClient"
```

---

## Task 6: Assistant service

**Files:**
- Modify: `src/assistant/service.rs` (replace stub)
- Create: `tests/assistant_test.rs`

- [ ] **Step 1: Write failing tests for Assistant**

Create `tests/assistant_test.rs`. This test mocks the LLM to return a simple text response and verifies the full flow:

```rust
use std::sync::Arc;
use serde_json::json;
use wisp::assistant::service::Assistant;
use wisp::platform::{Platform, ChatRequest};
use uuid::Uuid;

#[tokio::test]
#[ignore] // Requires database + wiremock
async fn assistant_handles_simple_chat() {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string());
    let pool = wisp::db::create_pool(&db_url).await.unwrap();
    wisp::db::run_migrations(&pool).await.unwrap();

    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/messages"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "content": [{"type": "text", "text": "Hi there!"}],
            "stop_reason": "end_turn"
        })))
        .mount(&mock_server)
        .await;

    let claude = Arc::new(wisp::llm::claude::ClaudeClient::new("test-key", &mock_server.uri()));
    let memory = Arc::new(wisp::db::memory::Memory::new(pool.clone()));
    let users = Arc::new(wisp::db::users::UserService::new(pool));
    let registry = wisp::tools::ToolRegistry::new();

    let assistant = Assistant::new(claude, memory, users, Arc::new(registry));

    let user_id = Uuid::new_v4();
    let request = ChatRequest {
        user_id,
        channel_id: "test-channel".to_string(),
        platform: Platform::Discord,
        message: "Hello".to_string(),
    };

    let response = assistant.handle(request).await.unwrap();
    assert_eq!(response.text, "Hi there!");
}

#[tokio::test]
#[ignore] // Requires database + wiremock
async fn assistant_handles_tool_use_loop() {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string());
    let pool = wisp::db::create_pool(&db_url).await.unwrap();
    wisp::db::run_migrations(&pool).await.unwrap();

    let mock_server = wiremock::MockServer::start().await;

    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/messages"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "content": [{"type": "text", "text": "The weather is sunny in Taipei."}],
            "stop_reason": "end_turn"
        })))
        .mount(&mock_server)
        .await;

    let claude = Arc::new(wisp::llm::claude::ClaudeClient::new("test-key", &mock_server.uri()));
    let memory = Arc::new(wisp::db::memory::Memory::new(pool.clone()));
    let users = Arc::new(wisp::db::users::UserService::new(pool));
    let registry = wisp::tools::ToolRegistry::new();

    let assistant = Assistant::new(claude, memory, users, Arc::new(registry));

    let user_id = Uuid::new_v4();
    let request = ChatRequest {
        user_id,
        channel_id: "test-tool".to_string(),
        platform: Platform::Discord,
        message: "What's the weather?".to_string(),
    };

    let response = assistant.handle(request).await.unwrap();
    assert!(!response.text.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test assistant_test -- --ignored 2>&1 | tail -5`
Expected: Compilation error — `Assistant` struct not implemented

- [ ] **Step 3: Implement Assistant**

Replace `src/assistant/service.rs`. Key design: accumulate `claude_messages: Vec<ClaudeMessage>` across tool call iterations so multi-turn tool context is preserved:

```rust
use std::sync::Arc;
use serde_json::Value;

use crate::db::memory::Memory;
use crate::db::users::UserService;
use crate::error::AppError;
use crate::llm::claude::{ClaudeClient, LlmResponse};
use crate::platform::{ChatMessage, ChatRequest, ChatResponse};
use crate::tools::ToolRegistry;

const MAX_TOOL_ITERATIONS: usize = 10;
const SYSTEM_PROMPT: &str = "You are Wisp, a helpful AI assistant. Keep responses concise.";

pub struct Assistant {
    claude: Arc<ClaudeClient>,
    memory: Arc<Memory>,
    users: Arc<UserService>,
    tools: Arc<ToolRegistry>,
}

impl Assistant {
    pub fn new(
        claude: Arc<ClaudeClient>,
        memory: Arc<Memory>,
        users: Arc<UserService>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            claude,
            memory,
            users,
            tools,
        }
    }

    pub async fn handle(&self, request: ChatRequest) -> Result<ChatResponse, AppError> {
        let platform_str = request.platform.as_str();

        // Get or create conversation
        let conv_id = self
            .memory
            .get_or_create_conversation(
                request.user_id,
                &request.channel_id,
                platform_str,
            )
            .await
            .map_err(AppError::Database)?;

        // Store user message
        self.memory
            .store_message(conv_id, "user", &request.message, None)
            .await
            .map_err(AppError::Database)?;

        // Load history
        let history = self
            .memory
            .load_recent_messages(conv_id, 20)
            .await
            .map_err(AppError::Database)?;

        // Build tool definitions
        let tool_defs = self.tools.tool_definitions();
        let tools_param = if tool_defs.is_empty() {
            None
        } else {
            Some(&tool_defs)
        };

        // Initial LLM call
        let mut response = self
            .claude
            .chat(&history, Some(SYSTEM_PROMPT), tools_param)
            .await?;

        // Tool call loop — accumulate messages for multi-turn context
        let mut accumulated_messages: Vec<ChatMessage> = history.clone();
        let mut tool_exchanges: Vec<(String, String, Value, String)> = Vec::new(); // (id, name, input, result)
        let mut iterations = 0;

        while let LlmResponse::ToolUse { id, name, input } = &response {
            iterations += 1;
            if iterations > MAX_TOOL_ITERATIONS {
                let text = "Sorry, I encountered too many tool calls. Please try again.".to_string();
                self.memory
                    .store_message(conv_id, "assistant", &text, None)
                    .await
                    .map_err(AppError::Database)?;
                return Ok(ChatResponse { text });
            }

            // Execute tool
            let tool_result = match self.tools.execute(name, input.clone()).await {
                Ok(result) => result,
                Err(e) => format!("Tool error: {e}"),
            };

            tool_exchanges.push((id.clone(), name.clone(), input.clone(), tool_result.clone()));

            // Send full context (history + all accumulated tool exchanges) back to LLM
            response = self
                .claude
                .chat_with_tool_results(
                    &accumulated_messages,
                    &tool_exchanges,
                    Some(SYSTEM_PROMPT),
                    tools_param,
                )
                .await?;
        }

        let text = match response {
            LlmResponse::Text(t) => t,
            _ => unreachable!(),
        };

        // Store assistant response (only final text, not intermediate tool calls)
        self.memory
            .store_message(conv_id, "assistant", &text, None)
            .await
            .map_err(AppError::Database)?;

        Ok(ChatResponse { text })
    }
}
```

- [ ] **Step 4: Update ClaudeClient — add `chat_with_tool_results` method**

This replaces the single-turn `chat_with_tool_result` from Task 5 with a multi-turn version. Add to `src/llm/claude.rs`:

```rust
    /// Send accumulated tool call results back to Claude.
    /// Appends all tool_use/tool_result pairs after the conversation history.
    pub async fn chat_with_tool_results(
        &self,
        history: &[ChatMessage],
        tool_exchanges: &[(String, String, Value, String)], // (id, name, input, result)
        system_prompt: Option<&str>,
        tools: Option<&Vec<Value>>,
    ) -> Result<LlmResponse, AppError> {
        let mut claude_messages: Vec<ClaudeMessage> = history
            .iter()
            .map(|m| ClaudeMessage {
                role: m.role.clone(),
                content: ClaudeContent::Text(m.content.clone()),
            })
            .collect();

        // Append each tool exchange pair
        for (id, name, input, result) in tool_exchanges {
            claude_messages.push(ClaudeMessage {
                role: "assistant".to_string(),
                content: ClaudeContent::Blocks(vec![serde_json::json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input,
                })]),
            });
            claude_messages.push(ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Blocks(vec![serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": result,
                })]),
            });
        }

        self.send_request(claude_messages, system_prompt, tools).await
    }
```

Also refactor `chat` and the existing `chat_with_tool_result` to share a `send_request` private method that builds `ClaudeRequest`, sends it, and parses the response. Remove `chat_with_tool_result` (replaced by `chat_with_tool_results`).

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test assistant_test -- --ignored 2>&1 | tail -5`
Expected: 2 tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/assistant/service.rs src/llm/claude.rs tests/assistant_test.rs
git commit -m "feat: implement Assistant with multi-turn tool call loop"
```

---

## Task 7: Move Discord modules to platform/discord/

**Files:**
- Modify: `src/platform/discord/verify.rs` (copy from `src/discord/verify.rs`)
- Modify: `src/platform/discord/webhook.rs` (copy from `src/discord/webhook.rs`)
- Modify: `src/platform/discord/handler.rs` (rewrite from `src/discord/interaction.rs`)
- Delete: `src/discord/` directory
- Modify: `src/lib.rs` (remove `pub mod discord`)
- Modify: `src/scheduler.rs` (update imports)
- Modify: `tests/discord_verify_test.rs` (update imports)
- Modify: `tests/discord_webhook_test.rs` (update imports)
- Modify: `tests/interaction_test.rs` (update imports + adapt to new handler)

- [ ] **Step 1: Copy verify.rs and webhook.rs to new location**

Copy `src/discord/verify.rs` → `src/platform/discord/verify.rs` (content unchanged).
Copy `src/discord/webhook.rs` → `src/platform/discord/webhook.rs` (content unchanged).

- [ ] **Step 2: Rewrite Discord handler**

Replace `src/platform/discord/handler.rs`. This handler now only does: verify signature → parse → defer → spawn background task that calls `assistant.handle()` → update deferred response:

```rust
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;

use super::verify::verify_signature;
use crate::assistant::service::Assistant;
use crate::db::users::UserService;
use crate::platform::{ChatRequest, Platform};

#[derive(Clone)]
pub struct DiscordState {
    pub public_key_hex: String,
    pub application_id: String,
    pub bot_token: String,
    pub assistant: Arc<Assistant>,
    pub users: Arc<UserService>,
}

pub fn routes(state: Arc<DiscordState>) -> Router {
    Router::new()
        .route("/interactions", post(handle_interaction))
        .with_state(state)
}

async fn handle_interaction(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Verify signature
    let signature = headers
        .get("X-Signature-Ed25519")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let timestamp = headers
        .get("X-Signature-Timestamp")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if verify_signature(&state.public_key_hex, signature, timestamp, &body).is_err() {
        return (StatusCode::UNAUTHORIZED, "Invalid signature").into_response();
    }

    let interaction: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response(),
    };

    let interaction_type = interaction["type"].as_u64().unwrap_or(0);

    match interaction_type {
        // PING
        1 => Json(json!({"type": 1})).into_response(),

        // APPLICATION_COMMAND
        2 => {
            let state = state.clone();
            let interaction = interaction.clone();
            tokio::spawn(async move {
                if let Err(e) = process_command(&state, &interaction).await {
                    tracing::error!("Failed to process command: {e}");
                    // Update deferred response with error message
                    let _ = send_error_followup(&state, &interaction).await;
                }
            });
            // Respond with DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE
            Json(json!({"type": 5})).into_response()
        }

        _ => (StatusCode::BAD_REQUEST, "Unknown interaction type").into_response(),
    }
}

async fn process_command(
    state: Arc<DiscordState>,
    interaction: Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let platform_user_id = interaction["member"]["user"]["id"]
        .as_str()
        .or_else(|| interaction["user"]["id"].as_str())
        .unwrap_or("unknown");
    let channel_id = interaction["channel_id"].as_str().unwrap_or("unknown");

    let user_message = interaction["data"]["options"]
        .as_array()
        .and_then(|opts| opts.first())
        .and_then(|opt| opt["value"].as_str())
        .unwrap_or("");

    let interaction_token = interaction["token"].as_str().unwrap_or("");

    // Resolve user identity
    let user_id = state.users.resolve_or_create("discord", platform_user_id).await?;

    // Build ChatRequest and delegate to Assistant
    let request = ChatRequest {
        user_id,
        channel_id: channel_id.to_string(),
        platform: Platform::Discord,
        message: user_message.to_string(),
    };

    let response = state.assistant.handle(request).await?;

    // Update deferred response via Discord API
    let client = twilight_http::Client::new(state.bot_token.clone());
    let app_id = twilight_model::id::Id::new(state.application_id.parse::<u64>()?);
    client
        .interaction(app_id)
        .update_response(interaction_token)
        .content(Some(&response.text))
        .await?;

    Ok(())
}

async fn send_error_followup(
    state: &DiscordState,
    interaction: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let interaction_token = interaction["token"].as_str().unwrap_or("");
    let client = twilight_http::Client::new(state.bot_token.clone());
    let app_id = twilight_model::id::Id::new(state.application_id.parse::<u64>()?);
    client
        .interaction(app_id)
        .update_response(interaction_token)
        .content(Some("Sorry, something went wrong processing your request."))
        .await?;
    Ok(())
}
```

- [ ] **Step 3: Update tests — change imports**

Update `tests/discord_verify_test.rs`: change `wisp::discord::verify` → `wisp::platform::discord::verify`.

Update `tests/discord_webhook_test.rs`: change `wisp::discord::webhook` → `wisp::platform::discord::webhook`.

Rewrite `tests/interaction_test.rs` to test ping/pong via the new handler. The ping response doesn't need Assistant/DB, but the handler requires full `DiscordState`. Use a minimal setup that only tests signature verification and ping:

```rust
use axum_test::TestServer;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;

fn create_test_app() -> (TestServer, SigningKey) {
    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());

    // For ping-only testing, we create a minimal router that
    // only handles signature verification + ping response.
    // Full integration tests with Assistant require a database.
    let state = std::sync::Arc::new(wisp::platform::discord::handler::DiscordPingState {
        public_key_hex,
    });
    let router = wisp::platform::discord::handler::ping_router(state);
    let server = TestServer::new(router).unwrap();
    (server, signing_key)
}

fn sign_request(key: &SigningKey, timestamp: &str, body: &[u8]) -> String {
    let mut msg = Vec::new();
    msg.extend_from_slice(timestamp.as_bytes());
    msg.extend_from_slice(body);
    hex::encode(key.sign(&msg).to_bytes())
}

#[tokio::test]
async fn ping_returns_pong() {
    let (server, key) = create_test_app();
    let body = json!({"type": 1}).to_string();
    let timestamp = "1234567890";
    let signature = sign_request(&key, timestamp, body.as_bytes());

    let resp = server
        .post("/interactions")
        .add_header("X-Signature-Ed25519".parse().unwrap(), signature.parse().unwrap())
        .add_header("X-Signature-Timestamp".parse().unwrap(), timestamp.parse().unwrap())
        .bytes(body.into())
        .await;

    resp.assert_status_ok();
    resp.assert_json(&json!({"type": 1}));
}

#[tokio::test]
async fn invalid_signature_returns_401() {
    let (server, _key) = create_test_app();
    let body = json!({"type": 1}).to_string();

    let resp = server
        .post("/interactions")
        .add_header("X-Signature-Ed25519".parse().unwrap(), "invalid".parse().unwrap())
        .add_header("X-Signature-Timestamp".parse().unwrap(), "1234567890".parse().unwrap())
        .bytes(body.into())
        .await;

    resp.assert_status_unauthorized();
}
```

Note: Add a `ping_router` helper and `DiscordPingState` to `src/platform/discord/handler.rs` for test-only use (similar to the existing `test_router` pattern):

```rust
/// Test-only state and router for ping/pong without full app dependencies.
#[derive(Clone)]
pub struct DiscordPingState {
    pub public_key_hex: String,
}

pub fn ping_router(state: Arc<DiscordPingState>) -> Router {
    Router::new()
        .route("/interactions", post(handle_ping_only))
        .with_state(state)
}

async fn handle_ping_only(
    State(state): State<Arc<DiscordPingState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let signature = headers.get("X-Signature-Ed25519").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let timestamp = headers.get("X-Signature-Timestamp").and_then(|v| v.to_str().ok()).unwrap_or_default();

    if verify_signature(&state.public_key_hex, signature, timestamp, &body).is_err() {
        return (StatusCode::UNAUTHORIZED, "Invalid signature").into_response();
    }

    let interaction: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response(),
    };

    if interaction["type"].as_u64() == Some(1) {
        Json(json!({"type": 1})).into_response()
    } else {
        (StatusCode::BAD_REQUEST, "Only PING supported in test mode").into_response()
    }
}
```

- [ ] **Step 4: Update scheduler**

Update `src/scheduler.rs` fully:

```rust
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};

use wisp::config::Config;
use wisp::platform::discord::webhook::WebhookClient;
use wisp::weather::cwa::CwaClient;

pub async fn start_scheduler(config: Arc<Config>) -> Result<JobScheduler, Box<dyn std::error::Error + Send + Sync>> {
    let sched = JobScheduler::new().await?;

    // Only schedule weather report if Discord webhook is configured
    if let Some(ref discord_config) = config.discord {
        let webhook_url = discord_config.webhook_url.clone();
        let cwa_api_key = config.cwa_api_key.clone();
        let cwa_location = config.cwa_location.clone();

        sched
            .add(Job::new_async("0 0 6 * * *", move |_uuid, _lock| {
                let url = webhook_url.clone();
                let key = cwa_api_key.clone();
                let loc = cwa_location.clone();
                Box::pin(async move {
                    if let Err(e) = send_weather_report(&key, &loc, &url).await {
                        tracing::error!("Weather report failed: {e}");
                    }
                })
            })?)
            .await?;
    }

    sched.start().await?;
    tracing::info!("Scheduler started");
    Ok(sched)
}

async fn send_weather_report(
    cwa_api_key: &str,
    cwa_location: &str,
    webhook_url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cwa = CwaClient::with_default_url(cwa_api_key);
    let forecast = cwa.fetch_forecast(cwa_location).await?;

    let webhook = WebhookClient::new(webhook_url);
    let title = format!("{} 天氣預報", forecast.location);
    let description = forecast.to_embed_description();
    webhook.send_embed(&title, &description, 0x00AAFF).await?;

    tracing::info!("Sent weather report for {cwa_location}");
    Ok(())
}
```

- [ ] **Step 5: Delete old discord module**

Delete `src/discord/` directory. Remove `pub mod discord;` from `src/lib.rs`.

- [ ] **Step 6: Run all tests to verify**

Run: `cargo test 2>&1 | tail -10`
Expected: All non-ignored tests PASS

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: move Discord modules to platform/discord/, rewrite handler to use Assistant"
```

---

## Task 8: Config — optional platform support

**Files:**
- Modify: `src/config.rs`
- Modify: `tests/config_test.rs`

- [ ] **Step 1: Write failing tests for optional config**

Update `tests/config_test.rs`. Note: `std::env::set_var` is `unsafe` in Rust 2024 edition. Use direct struct construction instead:

```rust
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
    assert!(config.discord.is_some());
    assert!(config.line.is_some());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test config_test 2>&1 | tail -5`
Expected: Compilation error — `config.discord` does not exist

- [ ] **Step 3: Update Config struct**

Update `src/config.rs`:

```rust
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
}

#[derive(Debug, Clone)]
pub struct DiscordConfig {
    pub application_id: String,
    pub public_key: String,
    pub bot_token: String,
    pub webhook_url: String,
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
        ) {
            (Ok(app_id), Ok(pub_key), Ok(bot_token), Ok(webhook_url)) => {
                Some(DiscordConfig {
                    application_id: app_id,
                    public_key: pub_key,
                    bot_token,
                    webhook_url,
                })
            }
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
        })
    }
}
```

- [ ] **Step 4: Verify scheduler already updated**

Scheduler was already fully updated in Task 7 Step 4. No changes needed here.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test config_test 2>&1 | tail -5`
Expected: 2 tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/config.rs src/scheduler.rs tests/config_test.rs
git commit -m "feat: make platform config optional, add LineConfig"
```

---

## Task 9: LINE Bot — signature verification and client

**Files:**
- Create: `src/platform/line/client.rs`
- Create: `src/platform/line/handler.rs`
- Modify: `src/platform/line/mod.rs`
- Create: `tests/line_client_test.rs`
- Create: `tests/line_handler_test.rs`
- Modify: `src/error.rs`
- Modify: `Cargo.toml` (add `hmac`, `sha2`)

- [ ] **Step 1: Add dependencies**

Add to `Cargo.toml` under `[dependencies]`:

```toml
hmac = "0.12"
sha2 = "0.10"
base64 = "0.22"
```

- [ ] **Step 2: Write failing tests for LINE signature verification**

Create `tests/line_handler_test.rs`:

```rust
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
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --test line_handler_test 2>&1 | tail -5`
Expected: Compilation error — function not found

- [ ] **Step 4: Write failing tests for LINE client**

Create `tests/line_client_test.rs`:

```rust
use wiremock::{MockServer, Mock, matchers, ResponseTemplate};
use wisp::platform::line::client::LineClient;

#[tokio::test]
async fn reply_message_sends_correct_request() {
    let mock_server = MockServer::start().await;

    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v2/bot/message/reply"))
        .and(matchers::header("Authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = LineClient::new("test-token", &mock_server.uri());
    client.reply("reply-token-123", "Hello from Wisp!").await.unwrap();
}

#[tokio::test]
async fn push_message_sends_correct_request() {
    let mock_server = MockServer::start().await;

    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v2/bot/message/push"))
        .and(matchers::header("Authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = LineClient::new("test-token", &mock_server.uri());
    client.push("user-id-123", "Fallback message").await.unwrap();
}
```

- [ ] **Step 5: Implement LINE signature verification**

Update `src/platform/line/handler.rs`:

```rust
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;
use std::sync::Arc;

use crate::assistant::service::Assistant;
use crate::db::users::UserService;
use crate::error::AppError;
use crate::platform::{ChatRequest, Platform};

#[derive(Clone)]
pub struct LineState {
    pub channel_secret: String,
    pub channel_access_token: String,
    pub assistant: Arc<Assistant>,
    pub users: Arc<UserService>,
    pub client: Arc<super::client::LineClient>,
}

pub fn routes(state: Arc<LineState>) -> Router {
    Router::new()
        .route("/webhook", post(handle_webhook))
        .with_state(state)
}

pub fn verify_line_signature(
    channel_secret: &str,
    signature: &str,
    body: &[u8],
) -> Result<(), AppError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(channel_secret.as_bytes())
        .map_err(|e| AppError::Internal(format!("HMAC error: {e}")))?;
    mac.update(body);

    let expected = BASE64.encode(mac.finalize().into_bytes());

    if signature == expected {
        Ok(())
    } else {
        Err(AppError::VerificationFailed)
    }
}

async fn handle_webhook(
    State(state): State<Arc<LineState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let signature = headers
        .get("X-Line-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if verify_line_signature(&state.channel_secret, signature, &body).is_err() {
        return StatusCode::UNAUTHORIZED;
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    // Process events in background
    let events = payload["events"].as_array().cloned().unwrap_or_default();
    for event in events {
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = process_event(state, event).await {
                tracing::error!("Failed to process LINE event: {e}");
            }
        });
    }

    StatusCode::OK
}

async fn process_event(
    state: Arc<LineState>,
    event: Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let event_type = event["type"].as_str().unwrap_or("");
    if event_type != "message" {
        return Ok(());
    }

    let message_type = event["message"]["type"].as_str().unwrap_or("");
    if message_type != "text" {
        return Ok(());
    }

    let platform_user_id = event["source"]["userId"].as_str().unwrap_or("unknown");
    let reply_token = event["replyToken"].as_str().unwrap_or("");
    let text = event["message"]["text"].as_str().unwrap_or("");

    // Determine channel_id based on source type
    let channel_id = event["source"]["groupId"]
        .as_str()
        .or_else(|| event["source"]["roomId"].as_str())
        .unwrap_or(platform_user_id);

    // Resolve user
    let user_id = state.users.resolve_or_create("line", platform_user_id).await?;

    let request = ChatRequest {
        user_id,
        channel_id: channel_id.to_string(),
        platform: Platform::Line,
        message: text.to_string(),
    };

    let response = state.assistant.handle(request).await?;

    // Try reply first, fall back to push if it fails
    if state.client.reply(reply_token, &response.text).await.is_err() {
        tracing::warn!("Reply token expired, using push message");
        state.client.push(platform_user_id, &response.text).await?;
    }

    Ok(())
}
```

- [ ] **Step 6: Implement LINE client**

Replace `src/platform/line/client.rs`:

```rust
use reqwest::Client;
use serde_json::json;

use crate::error::AppError;

pub struct LineClient {
    channel_access_token: String,
    base_url: String,
    http: Client,
}

impl LineClient {
    pub fn new(channel_access_token: &str, base_url: &str) -> Self {
        Self {
            channel_access_token: channel_access_token.to_string(),
            base_url: base_url.to_string(),
            http: Client::new(),
        }
    }

    pub fn with_default_url(channel_access_token: &str) -> Self {
        Self::new(channel_access_token, "https://api.line.me")
    }

    pub async fn reply(&self, reply_token: &str, text: &str) -> Result<(), AppError> {
        self.http
            .post(format!("{}/v2/bot/message/reply", self.base_url))
            .header("Authorization", format!("Bearer {}", self.channel_access_token))
            .header("Content-Type", "application/json")
            .json(&json!({
                "replyToken": reply_token,
                "messages": [{"type": "text", "text": text}]
            }))
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::Request)?;
        Ok(())
    }

    pub async fn push(&self, user_id: &str, text: &str) -> Result<(), AppError> {
        self.http
            .post(format!("{}/v2/bot/message/push", self.base_url))
            .header("Authorization", format!("Bearer {}", self.channel_access_token))
            .header("Content-Type", "application/json")
            .json(&json!({
                "to": user_id,
                "messages": [{"type": "text", "text": text}]
            }))
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::Request)?;
        Ok(())
    }
}
```

- [ ] **Step 7: Update platform/line/mod.rs**

```rust
pub mod client;
pub mod handler;
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test --test line_handler_test --test line_client_test 2>&1 | tail -5`
Expected: 4 tests PASS

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml src/platform/line/ src/error.rs tests/line_handler_test.rs tests/line_client_test.rs
git commit -m "feat: add LINE Bot webhook handler and Messaging API client"
```

---

## Task 10: Wire up main.rs

**Files:**
- Modify: `src/main.rs`
- Modify: `.env.example`

- [ ] **Step 1: Rewrite main.rs**

```rust
use std::sync::Arc;
use axum::{Router, routing::get};
use tracing_subscriber::EnvFilter;
use wisp::config::Config;
use wisp::assistant::service::Assistant;
use wisp::db::{create_pool, run_migrations};
use wisp::db::memory::Memory;
use wisp::db::users::UserService;
use wisp::llm::claude::ClaudeClient;
use wisp::platform::discord::handler::{DiscordState, routes as discord_routes};
use wisp::platform::line::client::LineClient;
use wisp::platform::line::handler::{LineState, routes as line_routes};
use wisp::tools::ToolRegistry;
use wisp::tools::weather::WeatherTool;
use wisp::weather::cwa::CwaClient;

mod scheduler;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    dotenvy::dotenv().ok();
    let config = Arc::new(Config::from_env().expect("Missing required environment variables"));

    // Database
    let pool = create_pool(&config.database_url)
        .await
        .expect("Failed to connect to database");
    run_migrations(&pool)
        .await
        .expect("Failed to run migrations");

    // Shared services
    let memory = Arc::new(Memory::new(pool.clone()));
    let users = Arc::new(UserService::new(pool));
    let claude = Arc::new(ClaudeClient::with_default_url(&config.anthropic_api_key));

    // Tool registry
    let mut registry = ToolRegistry::new();
    let cwa_client = CwaClient::with_default_url(&config.cwa_api_key);
    registry.register(Box::new(WeatherTool::new(cwa_client)));
    let tools = Arc::new(registry);

    let assistant = Arc::new(Assistant::new(
        claude,
        memory.clone(),
        users.clone(),
        tools,
    ));

    // Build router
    let mut app = Router::new().route("/health", get(|| async { "ok" }));

    // Discord (optional)
    if let Some(ref discord_config) = config.discord {
        let discord_state = Arc::new(DiscordState {
            public_key_hex: discord_config.public_key.clone(),
            application_id: discord_config.application_id.clone(),
            bot_token: discord_config.bot_token.clone(),
            assistant: assistant.clone(),
            users: users.clone(),
        });
        app = app.nest("/discord", discord_routes(discord_state));
        tracing::info!("Discord platform enabled");
    }

    // LINE (optional)
    if let Some(ref line_config) = config.line {
        let line_client = Arc::new(LineClient::with_default_url(&line_config.channel_access_token));
        let line_state = Arc::new(LineState {
            channel_secret: line_config.channel_secret.clone(),
            channel_access_token: line_config.channel_access_token.clone(),
            assistant: assistant.clone(),
            users: users.clone(),
            client: line_client,
        });
        app = app.nest("/line", line_routes(line_state));
        tracing::info!("LINE platform enabled");
    }

    // Scheduler
    let _scheduler = scheduler::start_scheduler(config.clone())
        .await
        .expect("Failed to start scheduler");

    // Start server
    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Starting Wisp on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

- [ ] **Step 2: Update .env.example**

Add LINE environment variables:

```
# LINE Bot (optional)
LINE_CHANNEL_SECRET=
LINE_CHANNEL_ACCESS_TOKEN=
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 4: Run all tests**

Run: `cargo test 2>&1 | tail -10`
Expected: All non-ignored tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/main.rs .env.example
git commit -m "feat: wire up multi-platform main.rs with optional Discord/LINE"
```

---

## Task 11: Update README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update README to reflect new architecture**

Update the project description, architecture section, tech stack table, and development phases. Key changes:
- Title: "基於 Rust 開發的高效能多平台 AI 助理服務"
- Architecture diagram showing Platform → Core → Tool layers
- Add LINE to tech stack
- Update route table
- Update development phases to reflect current state
- Reference design doc for details

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: update README for multi-platform assistant architecture"
```
