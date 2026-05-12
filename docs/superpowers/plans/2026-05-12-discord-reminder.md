# Discord Reminder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users set future reminders by chatting naturally in `allowed_channels` Discord channels. Wisp listens via Gateway, the LLM decides when to call `schedule_reminder`, and a 30s DB poller delivers the reminder as a non-pinging reply.

**Architecture:**
- Listen path: new Gateway shard (twilight) → filter allowed_channels → existing `Assistant::chat` with a new `schedule_reminder` tool → reply via REST.
- Deliver path: extend `scheduler.rs` with a 30s polling job that fetches due rows from a new `reminders` table and POSTs to `/channels/{id}/messages`.
- Tool calls need request context (user, channel, source message). Existing `Tool` trait is stateless; we refactor it to accept a `ToolContext`.

**Tech Stack:** Rust, sqlx, tokio, tokio-cron-scheduler, reqwest, twilight-gateway 0.16, twilight-model 0.16.

**Spec reference:** `docs/superpowers/specs/2026-05-12-discord-reminder-design.md`

---

## Task ordering & dependencies

```
T1 migration  ─►  T2 db/reminders.rs
                    │
                    └─►  T7 polling job ─►  T14 main.rs wiring
T3 Tool trait refactor  ─►  T4 ChatRequest ext ─►  T5 schedule_reminder tool ─►  T6 wire into main
T8 deps  ─►  T9 config  ─►  T10 gateway module  ─►  T11 filter  ─►  T12 dispatch  ─►  T13 rate limit/prompt  ─►  T14 main.rs
```

Independent batches: {T1, T2} ⫫ {T3, T4} ⫫ {T8, T9}.

---

## Task 1: Migration 006 — `reminders` schema

**Files:**
- Create: `migrations/006_reminders.sql`
- Modify: `src/db/mod.rs` — add the new migration to `run_migrations`

- [ ] **Step 1: Write the migration**

Create `migrations/006_reminders.sql`:

```sql
CREATE TABLE IF NOT EXISTS reminders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    platform        TEXT NOT NULL DEFAULT 'discord',
    guild_id        TEXT NOT NULL,
    channel_id      TEXT NOT NULL,
    source_message_id TEXT,
    user_id         UUID NOT NULL REFERENCES users(id),

    body            TEXT NOT NULL,
    fire_at         TIMESTAMPTZ NOT NULL,

    fired_at        TIMESTAMPTZ,
    failed_attempts INT NOT NULL DEFAULT 0,
    last_error      TEXT,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS reminders_due_idx
    ON reminders (fire_at)
    WHERE fired_at IS NULL;
```

- [ ] **Step 2: Register migration in `src/db/mod.rs`**

Append after the `005_tpp_webhooks.sql` block inside `run_migrations`:

```rust
    sqlx::raw_sql(include_str!("../../migrations/006_reminders.sql"))
        .execute(pool)
        .await?;
```

- [ ] **Step 3: Verify migration compiles & runs**

```
cargo check
DATABASE_URL=postgres://wisp:wisp@localhost:5432/wisp cargo test --test db_users_test -- --ignored resolve_or_create_creates_new_user
```

Expected: PASS (the test will trigger `run_migrations`, which now includes 006).

- [ ] **Step 4: Commit**

```
git add migrations/006_reminders.sql src/db/mod.rs
git commit -m "feat(db): add reminders table (migration 006)"
```

---

## Task 2: `db/reminders.rs` — CRUD layer

**Files:**
- Create: `src/db/reminders.rs`
- Modify: `src/db/mod.rs` — `pub mod reminders;`
- Create: `tests/db_reminders_test.rs`

- [ ] **Step 1: Write the failing tests**

Create `tests/db_reminders_test.rs`:

```rust
use chrono::{Duration, Utc};
use sqlx::PgPool;
use wisp::db::reminders::{NewReminder, Reminders};
use wisp::db::users::UserService;
use wisp::db::{create_pool, run_migrations};

async fn setup() -> (PgPool, uuid::Uuid) {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string());
    let pool = create_pool(&url).await.unwrap();
    run_migrations(&pool).await.unwrap();
    let users = UserService::new(pool.clone());
    let user_id = users
        .resolve_or_create("discord", &format!("test_{}", uuid::Uuid::new_v4()))
        .await
        .unwrap();
    (pool, user_id)
}

#[tokio::test]
#[ignore]
async fn insert_and_fetch_due() {
    let (pool, user_id) = setup().await;
    let repo = Reminders::new(pool.clone());

    let id = repo
        .insert(NewReminder {
            platform: "discord",
            guild_id: "g1",
            channel_id: "c1",
            source_message_id: Some("m1"),
            user_id,
            body: "buy milk",
            fire_at: Utc::now() - Duration::seconds(1),
        })
        .await
        .unwrap();

    let due = repo.fetch_due(10).await.unwrap();
    assert!(due.iter().any(|r| r.id == id), "inserted reminder should appear in due batch");
}

#[tokio::test]
#[ignore]
async fn future_reminder_not_due() {
    let (pool, user_id) = setup().await;
    let repo = Reminders::new(pool.clone());

    let id = repo
        .insert(NewReminder {
            platform: "discord",
            guild_id: "g1",
            channel_id: "c1",
            source_message_id: None,
            user_id,
            body: "future",
            fire_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap();

    let due = repo.fetch_due(10).await.unwrap();
    assert!(due.iter().all(|r| r.id != id));
}

#[tokio::test]
#[ignore]
async fn mark_fired_excludes_from_due() {
    let (pool, user_id) = setup().await;
    let repo = Reminders::new(pool.clone());

    let id = repo
        .insert(NewReminder {
            platform: "discord",
            guild_id: "g1",
            channel_id: "c1",
            source_message_id: None,
            user_id,
            body: "x",
            fire_at: Utc::now() - Duration::seconds(1),
        })
        .await
        .unwrap();

    repo.mark_fired(id).await.unwrap();

    let due = repo.fetch_due(10).await.unwrap();
    assert!(due.iter().all(|r| r.id != id));
}

#[tokio::test]
#[ignore]
async fn mark_failed_increments_attempts_and_excludes_after_5() {
    let (pool, user_id) = setup().await;
    let repo = Reminders::new(pool.clone());

    let id = repo
        .insert(NewReminder {
            platform: "discord",
            guild_id: "g1",
            channel_id: "c1",
            source_message_id: None,
            user_id,
            body: "x",
            fire_at: Utc::now() - Duration::seconds(1),
        })
        .await
        .unwrap();

    for _ in 0..5 {
        repo.mark_failed(id, "boom").await.unwrap();
    }

    let due = repo.fetch_due(10).await.unwrap();
    assert!(due.iter().all(|r| r.id != id), "after 5 failures it should be filtered out");
}
```

- [ ] **Step 2: Run tests — confirm failure**

```
cargo test --test db_reminders_test
```

Expected: compile errors (`wisp::db::reminders` not found).

- [ ] **Step 3: Write the implementation**

Create `src/db/reminders.rs`:

```rust
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewReminder<'a> {
    pub platform: &'a str,
    pub guild_id: &'a str,
    pub channel_id: &'a str,
    pub source_message_id: Option<&'a str>,
    pub user_id: Uuid,
    pub body: &'a str,
    pub fire_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Reminder {
    pub id: Uuid,
    pub platform: String,
    pub guild_id: String,
    pub channel_id: String,
    pub source_message_id: Option<String>,
    pub user_id: Uuid,
    pub body: String,
    pub fire_at: DateTime<Utc>,
    pub fired_at: Option<DateTime<Utc>>,
    pub failed_attempts: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct Reminders {
    pool: PgPool,
}

impl Reminders {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, r: NewReminder<'_>) -> Result<Uuid, sqlx::Error> {
        let row: (Uuid,) = sqlx::query_as(
            "INSERT INTO reminders
                (platform, guild_id, channel_id, source_message_id, user_id, body, fire_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id",
        )
        .bind(r.platform)
        .bind(r.guild_id)
        .bind(r.channel_id)
        .bind(r.source_message_id)
        .bind(r.user_id)
        .bind(r.body)
        .bind(r.fire_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn fetch_due(&self, limit: i64) -> Result<Vec<Reminder>, sqlx::Error> {
        sqlx::query_as::<_, Reminder>(
            "SELECT * FROM reminders
             WHERE fired_at IS NULL
               AND fire_at <= now()
               AND failed_attempts < 5
             ORDER BY fire_at
             LIMIT $1
             FOR UPDATE SKIP LOCKED",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn mark_fired(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE reminders SET fired_at = now() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_failed(&self, id: Uuid, error: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE reminders
             SET failed_attempts = failed_attempts + 1,
                 last_error = $2
             WHERE id = $1",
        )
        .bind(id)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
```

- [ ] **Step 4: Register module in `src/db/mod.rs`**

Add `pub mod reminders;` next to the other `pub mod` lines.

- [ ] **Step 5: Run tests — confirm pass**

```
cargo test --test db_reminders_test -- --ignored
```

Expected: 4 PASS.

- [ ] **Step 6: Commit**

```
git add src/db/reminders.rs src/db/mod.rs tests/db_reminders_test.rs
git commit -m "feat(db): add reminders CRUD with fetch_due / mark_fired / mark_failed"
```

---

## Task 3: Refactor `Tool` trait to accept `ToolContext`

Why: `schedule_reminder` needs request context (user_id, channel_id, guild_id, source_message_id). The current trait passes only `input: Value`. We add a `ToolContext` parameter; existing tools ignore it.

**Files:**
- Modify: `src/tools/mod.rs`
- Modify: `src/tools/time.rs`, `src/tools/weather.rs`, `src/tools/search.rs`
- Modify: `src/assistant/service.rs` — pass context through
- Modify: `tests/tools_registry_test.rs` — match new signature

- [ ] **Step 1: Define `ToolContext` and update trait in `src/tools/mod.rs`**

Replace the contents of `src/tools/mod.rs` with:

```rust
pub mod search;
pub mod time;
pub mod weather;

use std::collections::HashMap;
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use crate::error::AppError;
use crate::platform::Platform;

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub user_id: Uuid,
    pub platform: Platform,
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub source_message_id: Option<String>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, AppError>;
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

    pub async fn execute(
        &self,
        name: &str,
        input: Value,
        ctx: &ToolContext,
    ) -> Result<String, AppError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| AppError::Internal(format!("Unknown tool: {name}")))?;
        tool.execute(input, ctx).await
    }
}
```

- [ ] **Step 2: Update existing tools to ignore the new param**

In each of `src/tools/time.rs`, `src/tools/weather.rs`, `src/tools/search.rs`, change the signature of `execute`:

From:
```rust
async fn execute(&self, input: Value) -> Result<String, AppError> {
```
To:
```rust
async fn execute(&self, input: Value, _ctx: &super::ToolContext) -> Result<String, AppError> {
```

(For `time.rs` the parameter is `_input`; only adjust if you have to rename — otherwise keep as `_input`.)

- [ ] **Step 3: Update `Assistant::handle` to build and pass context**

In `src/assistant/service.rs`, near the top add:

```rust
use crate::tools::ToolContext;
```

Replace the tool execution line:

```rust
let tool_result = match self.tools.execute(name, input.clone()).await {
```

with:

```rust
let ctx = ToolContext {
    user_id: request.user_id,
    platform: request.platform,
    channel_id: request.channel_id.clone(),
    guild_id: None,            // will be populated after Task 4
    source_message_id: None,   // will be populated after Task 4
};
let tool_result = match self.tools.execute(name, input.clone(), &ctx).await {
```

- [ ] **Step 4: Update `tests/tools_registry_test.rs`**

Add `use wisp::tools::ToolContext;` and `use wisp::platform::Platform;`. Update the `EchoTool::execute` signature and the call sites:

```rust
async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<String, AppError> {
    Ok(input["text"].as_str().unwrap_or("").to_string())
}
```

For the two `registry.execute(...)` calls, build a stub context:

```rust
let ctx = ToolContext {
    user_id: uuid::Uuid::nil(),
    platform: Platform::Discord,
    channel_id: "test".into(),
    guild_id: None,
    source_message_id: None,
};
let result = registry.execute("echo", json!({"text": "hello"}), &ctx).await;
```

- [ ] **Step 5: Build + run all tests**

```
cargo test --tests
```

Expected: existing tests continue to pass (excluding the already-broken `assistant_test` and `llm_claude_test` listed in TODO T-004 / T-005).

- [ ] **Step 6: Commit**

```
git add src/tools src/assistant/service.rs tests/tools_registry_test.rs
git commit -m "refactor(tools): pass ToolContext to Tool::execute"
```

---

## Task 4: Extend `ChatRequest` and propagate context

**Files:**
- Modify: `src/platform/mod.rs`
- Modify: `src/assistant/service.rs`
- Modify: `src/platform/discord/handler.rs` (slash command path — pass `None`)
- Modify: `src/platform/line/handler.rs` (LINE path — pass `None`)

- [ ] **Step 1: Add optional fields to `ChatRequest` in `src/platform/mod.rs`**

```rust
pub struct ChatRequest {
    pub user_id: Uuid,
    pub channel_id: String,
    pub platform: Platform,
    pub message: String,
    pub guild_id: Option<String>,
    pub source_message_id: Option<String>,
}
```

- [ ] **Step 2: Populate context in `Assistant::handle`**

In `src/assistant/service.rs`, replace the placeholder `None` values in the `ToolContext` build (added in Task 3 Step 3):

```rust
let ctx = ToolContext {
    user_id: request.user_id,
    platform: request.platform,
    channel_id: request.channel_id.clone(),
    guild_id: request.guild_id.clone(),
    source_message_id: request.source_message_id.clone(),
};
```

- [ ] **Step 3: Update existing callers to fill `None`**

In `src/platform/discord/handler.rs` and `src/platform/line/handler.rs`, find every `ChatRequest { ... }` construction and add:

```rust
guild_id: None,
source_message_id: None,
```

(Slash command path doesn't have a source message; you may set `guild_id` from the interaction in a follow-up but it's not needed for this MVP.)

- [ ] **Step 4: Build**

```
cargo check
```

Expected: green.

- [ ] **Step 5: Commit**

```
git add src/platform src/assistant/service.rs
git commit -m "feat(platform): add optional guild_id and source_message_id to ChatRequest"
```

---

## Task 5: `schedule_reminder` tool

**Files:**
- Create: `src/tools/reminder.rs`
- Modify: `src/tools/mod.rs` — `pub mod reminder;`
- Create: `tests/tools_reminder_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/tools_reminder_test.rs`:

```rust
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use wisp::db::reminders::Reminders;
use wisp::db::users::UserService;
use wisp::db::{create_pool, run_migrations};
use wisp::platform::Platform;
use wisp::tools::reminder::ScheduleReminderTool;
use wisp::tools::{Tool, ToolContext};

async fn setup() -> (PgPool, Uuid) {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string());
    let pool = create_pool(&url).await.unwrap();
    run_migrations(&pool).await.unwrap();
    let users = UserService::new(pool.clone());
    let user_id = users
        .resolve_or_create("discord", &format!("rem_{}", Uuid::new_v4()))
        .await
        .unwrap();
    (pool, user_id)
}

fn ctx(user_id: Uuid) -> ToolContext {
    ToolContext {
        user_id,
        platform: Platform::Discord,
        channel_id: "c1".into(),
        guild_id: Some("g1".into()),
        source_message_id: Some("m1".into()),
    }
}

#[tokio::test]
#[ignore]
async fn happy_path_inserts_reminder() {
    let (pool, user_id) = setup().await;
    let tool = ScheduleReminderTool::new(pool.clone());

    let fire_at = (Utc::now() + Duration::minutes(10)).to_rfc3339();
    let result = tool
        .execute(json!({ "fire_at": fire_at, "body": "倒垃圾" }), &ctx(user_id))
        .await
        .unwrap();

    assert!(result.contains("reminder_id"));
    assert!(result.contains("倒垃圾"));

    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM reminders WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1);
}

#[tokio::test]
#[ignore]
async fn rejects_past_time() {
    let (pool, user_id) = setup().await;
    let tool = ScheduleReminderTool::new(pool);

    let past = (Utc::now() - Duration::minutes(1)).to_rfc3339();
    let err = tool
        .execute(json!({ "fire_at": past, "body": "x" }), &ctx(user_id))
        .await;
    assert!(err.is_err() || err.as_ref().unwrap().contains("error"));
}

#[tokio::test]
#[ignore]
async fn rejects_too_far_future() {
    let (pool, user_id) = setup().await;
    let tool = ScheduleReminderTool::new(pool);

    let far = (Utc::now() + Duration::days(400)).to_rfc3339();
    let err = tool
        .execute(json!({ "fire_at": far, "body": "x" }), &ctx(user_id))
        .await;
    assert!(err.is_err() || err.as_ref().unwrap().contains("error"));
}

#[tokio::test]
#[ignore]
async fn rejects_oversized_body() {
    let (pool, user_id) = setup().await;
    let tool = ScheduleReminderTool::new(pool);

    let fire_at = (Utc::now() + Duration::minutes(1)).to_rfc3339();
    let body = "x".repeat(501);
    let err = tool
        .execute(json!({ "fire_at": fire_at, "body": body }), &ctx(user_id))
        .await;
    assert!(err.is_err() || err.as_ref().unwrap().contains("error"));
}
```

- [ ] **Step 2: Run tests — confirm failure**

```
cargo test --test tools_reminder_test
```

Expected: compile error (`wisp::tools::reminder` not found).

- [ ] **Step 3: Write the implementation**

Create `src/tools/reminder.rs`:

```rust
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Asia::Taipei;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::db::reminders::{NewReminder, Reminders};
use crate::error::AppError;

use super::{Tool, ToolContext};

pub struct ScheduleReminderTool {
    repo: Reminders,
}

impl ScheduleReminderTool {
    pub fn new(pool: PgPool) -> Self {
        Self {
            repo: Reminders::new(pool),
        }
    }
}

#[derive(Deserialize)]
struct Args {
    fire_at: String,
    body: String,
}

#[async_trait]
impl Tool for ScheduleReminderTool {
    fn name(&self) -> &str {
        "schedule_reminder"
    }

    fn description(&self) -> &str {
        "為使用者排一個未來時間點的單次提醒。當使用者明確要求被提醒某件事在某時刻時呼叫。如果時間表達模糊（例如「等等」、「待會」），不要呼叫此 tool，先用文字回問清楚。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "fire_at": {
                    "type": "string",
                    "format": "date-time",
                    "description": "提醒觸發時間，RFC3339 UTC。例如 2026-05-12T05:30:00Z。注意：使用者通常給的是台灣時間（Asia/Taipei, UTC+8），你需要先換算成 UTC。"
                },
                "body": {
                    "type": "string",
                    "description": "提醒文字，最多 500 字。使用第二人稱（「你要...」）。"
                }
            },
            "required": ["fire_at", "body"]
        })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<String, AppError> {
        let args: Args = serde_json::from_value(input)
            .map_err(|e| AppError::Internal(format!("invalid args: {e}")))?;

        let fire_at = DateTime::parse_from_rfc3339(&args.fire_at)
            .map_err(|e| AppError::Internal(format!("invalid fire_at: {e}")))?
            .with_timezone(&Utc);

        let now = Utc::now();
        if fire_at < now - Duration::seconds(5) {
            return Ok(json!({ "error": "fire_at 必須是未來時間" }).to_string());
        }
        if fire_at > now + Duration::days(365) {
            return Ok(json!({ "error": "提醒時間不能超過一年後" }).to_string());
        }
        if args.body.chars().count() > 500 {
            return Ok(json!({ "error": "body 過長（>500 字）" }).to_string());
        }

        let guild_id = ctx
            .guild_id
            .as_deref()
            .ok_or_else(|| AppError::Internal("schedule_reminder requires guild_id".into()))?;

        let id = self
            .repo
            .insert(NewReminder {
                platform: ctx.platform.as_str(),
                guild_id,
                channel_id: &ctx.channel_id,
                source_message_id: ctx.source_message_id.as_deref(),
                user_id: ctx.user_id,
                body: &args.body,
                fire_at,
            })
            .await
            .map_err(AppError::Database)?;

        let local = fire_at.with_timezone(&Taipei).to_rfc3339();

        Ok(json!({
            "reminder_id": id,
            "fire_at_local": local,
            "body": args.body,
        })
        .to_string())
    }
}
```

Note: errors that should be visible to the LLM (validation problems) are returned as `Ok(json_with_error)` so the LLM can react and ask the user to retry. Internal/system errors (DB, malformed input) return `Err(AppError)`.

- [ ] **Step 4: Register module**

Add `pub mod reminder;` to `src/tools/mod.rs`.

- [ ] **Step 5: Add `chrono-tz` dep**

`chrono-tz` is needed for `Asia::Taipei`. Check `Cargo.toml` — if absent, add:

```toml
chrono-tz = "0.9"
```

- [ ] **Step 6: Run tests — confirm pass**

```
cargo test --test tools_reminder_test -- --ignored
```

Expected: 4 PASS.

- [ ] **Step 7: Commit**

```
git add src/tools/reminder.rs src/tools/mod.rs tests/tools_reminder_test.rs Cargo.toml Cargo.lock
git commit -m "feat(tools): add schedule_reminder tool"
```

---

## Task 6: Wire `schedule_reminder` into `main.rs`

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Register the tool**

After the `registry.register(Box::new(TimeTool::new()));` line in `src/main.rs`, add:

```rust
registry.register(Box::new(wisp::tools::reminder::ScheduleReminderTool::new(pool.clone())));
```

- [ ] **Step 2: Build**

```
cargo check
```

Expected: green.

- [ ] **Step 3: Commit**

```
git add src/main.rs
git commit -m "feat(main): register schedule_reminder tool"
```

---

## Task 7: Polling job — fire due reminders

**Files:**
- Modify: `src/scheduler.rs`
- Create: `tests/reminders_polling_test.rs`

The send leg uses an HTTP client pointed at a real Discord endpoint; for tests we extract the per-row logic so we can mock the send call.

- [ ] **Step 1: Refactor `scheduler.rs` signature to accept pool + bot_token**

Change the function signature in `src/scheduler.rs`:

```rust
pub async fn start_scheduler(
    config: Arc<Config>,
    pool: sqlx::PgPool,
) -> Result<JobScheduler, Box<dyn std::error::Error + Send + Sync>> {
```

(Add `use sqlx::PgPool;` to the imports.)

Also update `src/main.rs` to pass `pool.clone()` to `start_scheduler`. Both callers must compile.

- [ ] **Step 2: Add `fire_due_reminders` polling job inside `start_scheduler`**

After the weather job registration (still inside `start_scheduler`), add:

```rust
// Reminder polling: every 30 seconds
if let Some(ref discord_config) = config.discord {
    let bot_token = discord_config.bot_token.clone();
    let pool2 = pool.clone();
    sched
        .add(Job::new_async("0/30 * * * * *", move |_uuid, _lock| {
            let pool = pool2.clone();
            let bot_token = bot_token.clone();
            Box::pin(async move {
                if let Err(e) = fire_due_reminders(&pool, &bot_token).await {
                    tracing::error!("reminder fire batch failed: {e}");
                }
            })
        })?)
        .await?;
}
```

- [ ] **Step 3: Implement `fire_due_reminders` and `send_reminder`**

Append to `src/scheduler.rs`:

```rust
use wisp::db::reminders::{Reminder, Reminders};

pub(crate) async fn fire_due_reminders(
    pool: &sqlx::PgPool,
    bot_token: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let repo = Reminders::new(pool.clone());
    let due = repo.fetch_due(50).await?;

    for r in due {
        match send_reminder(bot_token, &r).await {
            Ok(()) => {
                repo.mark_fired(r.id).await?;
                tracing::info!(reminder_id = %r.id, "reminder fired");
            }
            Err(e) => {
                let _ = repo.mark_failed(r.id, &e.to_string()).await;
                tracing::warn!(reminder_id = %r.id, error = %e, "reminder send failed");
            }
        }
    }
    Ok(())
}

async fn send_reminder(
    bot_token: &str,
    r: &Reminder,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut body = serde_json::json!({
        "content": r.body,
        "allowed_mentions": { "parse": [] },
    });
    if let Some(ref msg_id) = r.source_message_id {
        body["message_reference"] = serde_json::json!({
            "message_id": msg_id,
            "fail_if_not_exists": false,
        });
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "https://discord.com/api/v10/channels/{}/messages",
            r.channel_id
        ))
        .header("Authorization", format!("Bot {bot_token}"))
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {text}").into());
    }
    Ok(())
}
```

- [ ] **Step 4: Write integration test**

Create `tests/reminders_polling_test.rs`:

```rust
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;
use wisp::db::reminders::{NewReminder, Reminders};
use wisp::db::users::UserService;
use wisp::db::{create_pool, run_migrations};

async fn setup() -> (PgPool, Uuid) {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string());
    let pool = create_pool(&url).await.unwrap();
    run_migrations(&pool).await.unwrap();
    let users = UserService::new(pool.clone());
    let user_id = users
        .resolve_or_create("discord", &format!("poll_{}", Uuid::new_v4()))
        .await
        .unwrap();
    (pool, user_id)
}

#[tokio::test]
#[ignore]
async fn fetch_due_returns_only_unfired_past_under_5_attempts() {
    let (pool, user_id) = setup().await;
    let repo = Reminders::new(pool.clone());

    let due_id = repo
        .insert(NewReminder {
            platform: "discord",
            guild_id: "g",
            channel_id: "c",
            source_message_id: None,
            user_id,
            body: "due",
            fire_at: Utc::now() - Duration::seconds(1),
        })
        .await
        .unwrap();

    repo.insert(NewReminder {
        platform: "discord",
        guild_id: "g",
        channel_id: "c",
        source_message_id: None,
        user_id,
        body: "future",
        fire_at: Utc::now() + Duration::hours(1),
    })
    .await
    .unwrap();

    let due = repo.fetch_due(100).await.unwrap();
    let ids: Vec<_> = due.iter().map(|r| r.id).collect();
    assert!(ids.contains(&due_id));
}
```

(The HTTP send path is not unit-tested — it's exercised manually in Task 14 verification.)

- [ ] **Step 5: Run tests**

```
cargo test --test reminders_polling_test -- --ignored
cargo build
```

Expected: PASS + green build.

- [ ] **Step 6: Commit**

```
git add src/scheduler.rs src/main.rs tests/reminders_polling_test.rs
git commit -m "feat(scheduler): poll and fire due reminders every 30s"
```

---

## Task 8: Add `twilight-gateway` dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add deps**

In `Cargo.toml` under `[dependencies]`:

```toml
twilight-gateway = "0.16"
twilight-model = "0.16"
```

- [ ] **Step 2: Verify it compiles**

```
cargo check
```

Expected: dependency downloads, no code changes yet, green build.

- [ ] **Step 3: Commit**

```
git add Cargo.toml Cargo.lock
git commit -m "chore(deps): add twilight-gateway 0.16"
```

---

## Task 9: Config — bot user id and gateway toggle

**Files:**
- Modify: `src/config.rs`
- Modify: `tests/config_test.rs` (if existing tests reference DiscordConfig fields exhaustively)

- [ ] **Step 1: Extend `DiscordConfig`**

```rust
#[derive(Debug, Clone)]
pub struct DiscordConfig {
    pub application_id: String,
    pub public_key: String,
    pub bot_token: String,
    pub bot_user_id: String,
    pub webhook_url: String,
    pub client_secret: String,
    pub oauth_redirect_uri: String,
    pub state_secret: String,
    pub gateway_enabled: bool,
}
```

- [ ] **Step 2: Read new env vars**

In `from_env`, change the Discord tuple destructure to include `bot_user_id` (required) and read `gateway_enabled` from env (default true):

```rust
let discord = match (
    env::var("DISCORD_APPLICATION_ID"),
    env::var("DISCORD_PUBLIC_KEY"),
    env::var("DISCORD_BOT_TOKEN"),
    env::var("DISCORD_BOT_USER_ID"),
    env::var("DISCORD_WEBHOOK_URL"),
    env::var("DISCORD_CLIENT_SECRET"),
    env::var("DISCORD_OAUTH_REDIRECT_URI"),
    env::var("TPP_STATE_SECRET"),
) {
    (
        Ok(app_id),
        Ok(pub_key),
        Ok(bot_token),
        Ok(bot_user_id),
        Ok(webhook_url),
        Ok(client_secret),
        Ok(oauth_redirect_uri),
        Ok(state_secret),
    ) => Some(DiscordConfig {
        application_id: app_id,
        public_key: pub_key,
        bot_token,
        bot_user_id,
        webhook_url,
        client_secret,
        oauth_redirect_uri,
        state_secret,
        gateway_enabled: env::var("DISCORD_GATEWAY_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true),
    }),
    _ => None,
};
```

- [ ] **Step 3: Build & run config tests**

```
cargo test --test config_test
```

If `config_test.rs` checks specific fields and fails, update it to set `DISCORD_BOT_USER_ID` and assert the new field.

- [ ] **Step 4: Commit**

```
git add src/config.rs tests/config_test.rs
git commit -m "feat(config): add DISCORD_BOT_USER_ID and DISCORD_GATEWAY_ENABLED"
```

---

## Task 10: Gateway listener skeleton

**Files:**
- Create: `src/platform/discord/gateway.rs`
- Modify: `src/platform/discord/mod.rs` — `pub mod gateway;`

- [ ] **Step 1: Write the listener with a connect loop**

Create `src/platform/discord/gateway.rs`:

```rust
use std::sync::Arc;

use twilight_gateway::{Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt};
use twilight_model::gateway::payload::incoming::MessageCreate;

use crate::assistant::service::Assistant;
use crate::db::allowed_channels::AllowedChannels;
use crate::db::users::UserService;

pub struct GatewayConfig {
    pub bot_token: String,
    pub bot_user_id: String,
}

pub struct GatewayDeps {
    pub assistant: Arc<Assistant>,
    pub users: Arc<UserService>,
    pub allowed_channels: Arc<AllowedChannels>,
    pub bot_token: String,
}

pub async fn run(config: GatewayConfig, deps: Arc<GatewayDeps>) {
    let intents = Intents::GUILDS | Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT;
    let mut shard = Shard::new(ShardId::ONE, config.bot_token.clone(), intents);

    tracing::info!("Discord gateway shard starting");

    while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
        let event = match item {
            Ok(event) => event,
            Err(e) => {
                tracing::warn!("gateway recv error: {e}");
                continue;
            }
        };

        if let Event::MessageCreate(msg) = event {
            let deps = deps.clone();
            let bot_user_id = config.bot_user_id.clone();
            tokio::spawn(async move {
                if let Err(e) = super::listen::handle_message(*msg, &bot_user_id, deps).await {
                    tracing::error!("listen handler failed: {e}");
                }
            });
        }
    }

    tracing::warn!("Discord gateway shard ended");
}
```

Note: this references `super::listen::handle_message` which we add in Task 11. We intentionally split filter (Task 11, unit-testable) from dispatch.

- [ ] **Step 2: Register module**

In `src/platform/discord/mod.rs`, add:

```rust
pub mod gateway;
pub mod listen;
```

(`listen` will be created in Task 11; declaring it now keeps the order natural — but if you want strict TDD, defer the declaration until Task 11.)

- [ ] **Step 3: Build**

```
cargo check
```

Expected: error referencing `listen` module (will be fixed in Task 11). If you deferred the `pub mod listen;` declaration, expect green. Either is fine; if not green, proceed to Task 11.

- [ ] **Step 4: Commit (defer if not green)**

```
git add src/platform/discord/gateway.rs src/platform/discord/mod.rs
git commit -m "feat(discord): add gateway listener skeleton"
```

If build is not yet green, defer commit until Task 11.

---

## Task 11: Listen filter & pipeline

**Files:**
- Create: `src/platform/discord/listen.rs`
- Create: `tests/discord_listen_test.rs`

- [ ] **Step 1: Write failing tests for the pure filter**

Create `tests/discord_listen_test.rs`:

```rust
use wisp::platform::discord::listen::should_process;

#[test]
fn drops_own_message() {
    assert!(!should_process(
        /* author_id */ "bot123",
        /* author_is_bot */ false,
        /* content */ "hello",
        /* bot_user_id */ "bot123",
    ));
}

#[test]
fn drops_bot_messages() {
    assert!(!should_process("other_bot", true, "hello", "bot123"));
}

#[test]
fn drops_empty_content() {
    assert!(!should_process("user1", false, "   ", "bot123"));
}

#[test]
fn allows_human_message() {
    assert!(should_process("user1", false, "hello", "bot123"));
}
```

- [ ] **Step 2: Run tests — confirm failure**

```
cargo test --test discord_listen_test
```

Expected: compile error.

- [ ] **Step 3: Implement `listen.rs`**

Create `src/platform/discord/listen.rs`:

```rust
use std::sync::Arc;

use twilight_model::gateway::payload::incoming::MessageCreate;

use crate::error::AppError;
use crate::platform::{ChatRequest, Platform};

use super::gateway::GatewayDeps;

/// Pure filter — pre-pipeline drop decisions.
pub fn should_process(
    author_id: &str,
    author_is_bot: bool,
    content: &str,
    bot_user_id: &str,
) -> bool {
    if author_id == bot_user_id {
        return false;
    }
    if author_is_bot {
        return false;
    }
    if content.trim().is_empty() {
        return false;
    }
    true
}

pub async fn handle_message(
    msg: MessageCreate,
    bot_user_id: &str,
    deps: Arc<GatewayDeps>,
) -> Result<(), AppError> {
    let author_id = msg.author.id.to_string();
    let author_is_bot = msg.author.bot;
    let content = msg.content.clone();

    if !should_process(&author_id, author_is_bot, &content, bot_user_id) {
        return Ok(());
    }

    let guild_id = match msg.guild_id {
        Some(g) => g.to_string(),
        None => return Ok(()), // DMs not supported in listen mode
    };
    let channel_id = msg.channel_id.to_string();

    if !deps
        .allowed_channels
        .is_public(&guild_id, &channel_id)
        .await
    {
        return Ok(());
    }

    let user_id = deps
        .users
        .resolve_or_create("discord", &author_id)
        .await
        .map_err(AppError::Database)?;

    let response = deps
        .assistant
        .handle(ChatRequest {
            user_id,
            channel_id: channel_id.clone(),
            platform: Platform::Discord,
            message: content,
            guild_id: Some(guild_id),
            source_message_id: Some(msg.id.to_string()),
        })
        .await?;

    // LLM short-circuit: if response is empty, do not reply.
    if response.text.trim().is_empty() {
        return Ok(());
    }

    // Send reply via REST, replying to source message, no mentions.
    send_reply(
        &deps.bot_token,
        &channel_id,
        &msg.id.to_string(),
        &response.text,
    )
    .await?;

    Ok(())
}

async fn send_reply(
    bot_token: &str,
    channel_id: &str,
    source_message_id: &str,
    content: &str,
) -> Result<(), AppError> {
    let body = serde_json::json!({
        "content": content,
        "allowed_mentions": { "parse": [] },
        "message_reference": {
            "message_id": source_message_id,
            "fail_if_not_exists": false,
        }
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "https://discord.com/api/v10/channels/{channel_id}/messages"
        ))
        .header("Authorization", format!("Bot {bot_token}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("discord reply failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "discord reply HTTP {status}: {text}"
        )));
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests**

```
cargo test --test discord_listen_test
cargo check
```

Expected: 4 PASS, green build.

- [ ] **Step 5: Commit (combined with Task 10 if deferred)**

```
git add src/platform/discord/listen.rs src/platform/discord/gateway.rs src/platform/discord/mod.rs tests/discord_listen_test.rs
git commit -m "feat(discord): listen pipeline + filter + reply"
```

---

## Task 12: Per-user rate limiter

**Files:**
- Create: `src/platform/discord/rate_limit.rs`
- Modify: `src/platform/discord/mod.rs` — `pub mod rate_limit;`
- Modify: `src/platform/discord/listen.rs` — integrate
- Create: `tests/discord_rate_limit_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/discord_rate_limit_test.rs`:

```rust
use std::time::Duration;
use wisp::platform::discord::rate_limit::RateLimiter;

#[tokio::test]
async fn allows_first_call() {
    let rl = RateLimiter::new(Duration::from_secs(60));
    assert!(rl.allow("u1"));
}

#[tokio::test]
async fn blocks_second_call_within_window() {
    let rl = RateLimiter::new(Duration::from_secs(60));
    assert!(rl.allow("u1"));
    assert!(!rl.allow("u1"));
}

#[tokio::test]
async fn different_users_independent() {
    let rl = RateLimiter::new(Duration::from_secs(60));
    assert!(rl.allow("u1"));
    assert!(rl.allow("u2"));
}

#[tokio::test]
async fn allows_after_window_elapses() {
    let rl = RateLimiter::new(Duration::from_millis(50));
    assert!(rl.allow("u1"));
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(rl.allow("u1"));
}
```

- [ ] **Step 2: Run tests — confirm failure**

```
cargo test --test discord_rate_limit_test
```

Expected: compile error.

- [ ] **Step 3: Implement**

Create `src/platform/discord/rate_limit.rs`:

```rust
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct RateLimiter {
    window: Duration,
    last: Mutex<HashMap<String, Instant>>,
}

impl RateLimiter {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            last: Mutex::new(HashMap::new()),
        }
    }

    pub fn allow(&self, user_id: &str) -> bool {
        let mut map = self.last.lock().unwrap();
        let now = Instant::now();
        match map.get(user_id) {
            Some(&t) if now.duration_since(t) < self.window => false,
            _ => {
                map.insert(user_id.to_string(), now);
                true
            }
        }
    }
}
```

- [ ] **Step 4: Wire into `listen.rs`**

In `src/platform/discord/listen.rs`, modify `GatewayDeps` in `gateway.rs` (Task 10) to include:

```rust
pub rate_limiter: Arc<crate::platform::discord::rate_limit::RateLimiter>,
```

Then in `handle_message`, after the `should_process` check and before any DB / LLM work, add:

```rust
if !deps.rate_limiter.allow(&author_id) {
    tracing::debug!(user = %author_id, "listen rate-limited");
    return Ok(());
}
```

- [ ] **Step 5: Module registration**

Add `pub mod rate_limit;` to `src/platform/discord/mod.rs`.

- [ ] **Step 6: Run tests**

```
cargo test --test discord_rate_limit_test
cargo check
```

Expected: 4 PASS, green build.

- [ ] **Step 7: Commit**

```
git add src/platform/discord/rate_limit.rs src/platform/discord/listen.rs src/platform/discord/gateway.rs src/platform/discord/mod.rs tests/discord_rate_limit_test.rs
git commit -m "feat(discord): per-user listen rate limit (1/60s)"
```

---

## Task 13: Listen-mode system prompt addition

**Files:**
- Modify: `src/assistant/service.rs`

The listen path needs an extra prompt directive. Easiest: a per-request prompt override.

- [ ] **Step 1: Add a `listen_mode` flag to `ChatRequest`**

In `src/platform/mod.rs`:

```rust
pub struct ChatRequest {
    pub user_id: Uuid,
    pub channel_id: String,
    pub platform: Platform,
    pub message: String,
    pub guild_id: Option<String>,
    pub source_message_id: Option<String>,
    pub listen_mode: bool,
}
```

- [ ] **Step 2: Compose the prompt in `Assistant::handle`**

In `src/assistant/service.rs`, replace the line:

```rust
let system_prompt = system_prompt_for(request.platform);
```

with:

```rust
let base_prompt = system_prompt_for(request.platform);
let system_prompt_string: String;
let system_prompt: &str = if request.listen_mode {
    system_prompt_string = format!("{base_prompt}\n\n## Listen 模式\n\
你正在「被動聆聽」模式下監聽頻道訊息。\n\
- 只在訊息明確需要回應、或包含可執行的請求（如設定提醒）時才回覆\n\
- 閒聊、與你無關的對話、別人之間的討論，回**空字串**\n\
- 用 schedule_reminder 排提醒前先確認時間表達清楚；不確定就先問");
    &system_prompt_string
} else {
    base_prompt
};
```

- [ ] **Step 3: Update existing callers**

- `src/platform/discord/handler.rs`: add `listen_mode: false`
- `src/platform/line/handler.rs`: add `listen_mode: false`
- `src/platform/discord/listen.rs`: set `listen_mode: true`

- [ ] **Step 4: Build**

```
cargo check
```

Expected: green.

- [ ] **Step 5: Commit**

```
git add src/platform src/assistant/service.rs
git commit -m "feat(assistant): listen-mode system prompt addendum"
```

---

## Task 14: Wire gateway task into `main.rs`

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Spawn gateway task when enabled**

In `src/main.rs`, after the Discord-enabled block (after `app = app.nest("/discord", ...)`), add:

```rust
if discord_config.gateway_enabled {
    let gateway_config = wisp::platform::discord::gateway::GatewayConfig {
        bot_token: discord_config.bot_token.clone(),
        bot_user_id: discord_config.bot_user_id.clone(),
    };
    let rate_limiter = Arc::new(
        wisp::platform::discord::rate_limit::RateLimiter::new(
            std::time::Duration::from_secs(60),
        ),
    );
    let gateway_deps = Arc::new(wisp::platform::discord::gateway::GatewayDeps {
        assistant: assistant.clone(),
        users: users.clone(),
        allowed_channels: allowed_channels.clone(),
        bot_token: discord_config.bot_token.clone(),
        rate_limiter,
    });
    tokio::spawn(wisp::platform::discord::gateway::run(gateway_config, gateway_deps));
    tracing::info!("Discord gateway enabled");
}
```

- [ ] **Step 2: Build**

```
cargo build
```

Expected: green.

- [ ] **Step 3: Manual end-to-end verification**

In `.env`, set `DISCORD_BOT_USER_ID` to your bot's user ID (from Developer Portal).
In Discord Developer Portal → **Bot → Privileged Gateway Intents → enable MESSAGE CONTENT INTENT**.

Start the bot:

```
cargo run
```

Expected log lines:
```
Discord gateway shard starting
```

In an `allowed_channels` channel of a guild where the bot is installed, type:

```
提醒我 2 分鐘後寫週報
```

Expected:
- Wisp replies (within seconds) with something like "好，我會在 HH:MM 提醒你寫週報" (model output)
- After ~2 minutes, Wisp posts a reply to your original message containing "寫週報" (no @ mention)
- DB: `SELECT id, body, fire_at, fired_at FROM reminders ORDER BY created_at DESC LIMIT 1;` — `fired_at` is non-NULL after firing

- [ ] **Step 4: Commit**

```
git add src/main.rs
git commit -m "feat(discord): spawn gateway listener task on startup"
```

---

## Task 15: Documentation updates

**Files:**
- Modify: `CHANGELOG.md`
- Modify: `docs/TODO.md` — remove items that the deferred set captures and add T-010+ for follow-ups
- Modify: `SETUP.md` or `README.md` — add the new env vars

- [ ] **Step 1: Update `CHANGELOG.md`**

Add a dated entry under the current month:

```markdown
### 新增
- **Discord 提醒功能** — 在允許頻道內用自然語言設提醒（如「10 分鐘後提醒我...」），到時 Wisp 自動 reply 通知。基於 Gateway WebSocket 監聽 + LLM tool + DB polling。
```

- [ ] **Step 2: Update `docs/TODO.md`**

Add follow-ups documented in spec §10:

```markdown
- [ ] T-010 **提醒功能：取消 / 列表 / 重複** — 加 `cancel_reminder`、`list_reminders` tool；schema 加 `recurrence` 欄位
- [ ] T-011 **提醒功能：成本 hard cap** — 單 guild 每日 LLM 呼叫上限，避免 listen 模式被洗版
- [ ] T-012 **提醒功能：LINE 平台** — `reminders.platform` 已預留欄位；LINE 無 Gateway，需另寫 webhook → listen pipeline bridge
```

- [ ] **Step 3: Update `SETUP.md` / `README.md`**

Add to the env var documentation section:

```
DISCORD_BOT_USER_ID         Bot's own user ID (from Developer Portal). Required to filter own messages in listen mode.
DISCORD_GATEWAY_ENABLED     Optional, default true. Set to "false" to disable the listen-mode WebSocket (e.g. local dev without intent approval).
```

Note: Developer Portal → **Bot → Privileged Gateway Intents → enable MESSAGE CONTENT INTENT**.

- [ ] **Step 4: Commit**

```
git add CHANGELOG.md docs/TODO.md SETUP.md README.md
git commit -m "docs: changelog + setup notes for Discord reminders"
```

---

## Verification checklist (post-implementation)

- [ ] All new tests pass: `cargo test --tests` and `cargo test --tests -- --ignored` (the ignored ones need DATABASE_URL)
- [ ] `cargo build --release` succeeds
- [ ] Manual test (Task 14 Step 3) passes — reminder fires within 30s of `fire_at`
- [ ] Bot restart: insert a reminder, kill bot, restart, verify reminder still fires
- [ ] `allowed_channels` filter: send "提醒我..." in a non-allowed channel → no LLM call (verify via logs / token_usage absence)
- [ ] Rate limit: spam 3 messages from same user within 60s → only 1 triggers LLM (verify via logs)
- [ ] Source message deleted: insert reminder, delete the source message, wait for fire — reminder still posts (`fail_if_not_exists: false`)
