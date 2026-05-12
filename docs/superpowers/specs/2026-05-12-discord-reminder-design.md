# Discord Reminder — Design Spec

- 日期：2026-05-12
- 狀態：Draft（待 user review → 進 implementation plan）
- 範圍：Wisp Discord 平台新增「未來時間點提醒」功能 MVP

## 1. 背景與目標

使用者希望在 Discord 已安裝 Wisp 的伺服器、且在 `allowed_channels` 白名單頻道內，**用自然語言**請 Wisp 在未來某時間點提醒某件事，到時 Wisp 自動發訊息到該頻道。

### 1.1 使用者場景

> User（在白名單頻道內發）：「Wisp 提醒我 10 分鐘後要倒垃圾」
> Wisp（立即）：「好，我會在 18:25 提醒你倒垃圾」
> ⋯⋯ 10 分鐘後 ⋯⋯
> Wisp（reply 到原訊息、不 ping）：「提醒：倒垃圾」

### 1.2 非目標（MVP 排除）

- 取消提醒
- 列出我所有提醒
- 重複提醒（cron-like）
- LINE 平台同等功能（schema 預留欄位、不實作）
- 跨 user 共享提醒脈絡

## 2. 已決定的設計選項

| 議題 | 決定 |
|---|---|
| 觸發機制 | Gateway listen message（新加 WebSocket 連線 + Message Content intent） |
| 範圍 | 限 `allowed_channels` 白名單頻道 |
| LLM 介入 | 白名單頻道內**所有訊息**送 LLM、由 LLM 判斷要不要回 / 要不要排程 |
| 設提醒 | LLM tool `schedule_reminder` |
| 提醒送出 | 純文 Discord reply、`allowed_mentions.parse=[]`（不 ping） |
| 觸發精度 | DB polling 每 30 秒，最大延遲 30 秒 |
| 持久化 | Postgres `reminders` 表，bot 重啟自然恢復 |
| 多 instance | 預埋 `FOR UPDATE SKIP LOCKED`，MVP 仍假設單 instance |

## 3. 架構

```
                    ┌───────────────────────────────────┐
                    │  Discord Gateway (WSS)            │
                    └──────────────┬────────────────────┘
                                   │ MESSAGE_CREATE event
                                   ▼
   ┌───────────────────────────────────────────────┐
   │  src/platform/discord/gateway.rs (NEW)        │
   │   - twilight-gateway shard                    │
   │   - filter: allowed_channels                  │
   │   - reply via REST                            │
   └──────────────┬────────────────────────────────┘
                  │
                  │ (history + system_prompt + tools)
                  ▼
   ┌───────────────────────────────────────────────┐
   │  Assistant::chat (EXISTING)                   │
   │   tools += schedule_reminder (NEW)            │
   └──────────────┬────────────────────────────────┘
                  │
                  │ on tool call → INSERT reminders
                  ▼
   ┌───────────────────────────────────────────────┐
   │  Postgres `reminders` (NEW migration 006)     │
   └──────────────┬────────────────────────────────┘
                  ▲
                  │ SELECT due reminders, every 30s
                  │
   ┌──────────────┴────────────────────────────────┐
   │  scheduler.rs (EXTEND)                        │
   │   tokio-cron-scheduler job                    │
   │   → REST POST channel message                 │
   │   → mark fired_at                             │
   └───────────────────────────────────────────────┘
```

### 3.1 模組變更清單

| 路徑 | 動作 | 說明 |
|---|---|---|
| `src/platform/discord/gateway.rs` | NEW | twilight-gateway listener + 訊息分派 |
| `src/platform/discord/mod.rs` | EDIT | 暴露 gateway module |
| `src/tools/reminder.rs` | NEW | `schedule_reminder` tool 實作 |
| `src/assistant/service.rs` | EDIT | 註冊新 tool、傳遞 `ToolContext` 擴充欄位 |
| `src/db/reminders.rs` | NEW | reminders CRUD |
| `src/scheduler.rs` | EDIT | 加 30s polling job 觸發提醒 |
| `migrations/006_reminders.sql` | NEW | schema |
| `src/config.rs` | EDIT | `DISCORD_BOT_USER_ID`, `DISCORD_GATEWAY_ENABLED` |
| `src/main.rs` | EDIT | spawn gateway task |
| `Cargo.toml` | EDIT | 加 `twilight-gateway`、`twilight-model` |

### 3.2 併發模型

Gateway shard、polling job、既有 axum server 各為獨立 tokio task；它們**只透過 Postgres 通訊**，互不共享 in-memory state，避免 lock 設計複雜化。

## 4. 資料模型

`migrations/006_reminders.sql`：

```sql
CREATE TABLE IF NOT EXISTS reminders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    platform        TEXT NOT NULL DEFAULT 'discord',
    guild_id        TEXT NOT NULL,
    channel_id      TEXT NOT NULL,
    source_message_id TEXT,                   -- reply target；可空（其他平台共用 schema 時不一定有）
    user_id         UUID NOT NULL REFERENCES users(id),

    body            TEXT NOT NULL,
    fire_at         TIMESTAMPTZ NOT NULL,

    fired_at        TIMESTAMPTZ,              -- NULL = 尚未觸發
    failed_attempts INT NOT NULL DEFAULT 0,
    last_error      TEXT,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS reminders_due_idx
    ON reminders (fire_at)
    WHERE fired_at IS NULL;
```

設計取捨：

- `fired_at` 用 timestamp 而非 boolean — 同欄位同時表示「狀態 + 實際送出時間」
- 沒有 `cancelled_at` / `recurrence` — MVP 排除
- 沒有 mention target — 確認過「純文 reply 不 ping」
- partial index 只索引未觸發列，polling query 在大表上仍常數成本

## 5. Listen Pipeline

### 5.1 訊息流程

```
Gateway MESSAGE_CREATE
  │
  ├─ 1. author.id == DISCORD_BOT_USER_ID?       → drop
  ├─ 2. author.bot == true?                     → drop
  ├─ 3. allowed_channels match?
  │      channel_id IS NULL → 整個 guild 允許
  │      channel_id 有值   → 必須完全 match
  │   不在 → drop
  ├─ 4. content 空白？                           → drop
  ├─ 5. ensure_user：從 platform_identities 找 / 建 users 列
  ├─ 6. rate_limit check（單 user 60s 1 次）     → 超過則 drop
  ├─ 7. Assistant::chat(history, msg, tools=[..., schedule_reminder])
  │      system_prompt = 既有 Discord prompt + listen 模式補充段
  ├─ 8. LLM 回 "" / 空白 → drop（不發訊息）
  └─ 9. 否則 → POST /channels/{id}/messages（reply 樣式、不 ping）
```

### 5.2 Listen 模式 system prompt 補充

於既有 Discord system prompt 後追加：

> 你正在「被動聆聽」模式下監聽頻道訊息。
> - 只在訊息明確需要回應、或包含可執行的請求（如設定提醒）時才回覆
> - 閒聊、與你無關的對話、別人之間的討論，回**空字串**
> - 用 `schedule_reminder` 排提醒前先確認時間表達清楚；不確定就先問

回空字串 → pipeline short-circuit，比指示 LLM「請保持安靜」可靠。

### 5.3 權限模型

| 層 | 機制 |
|---|---|
| Discord | Bot 需 `MESSAGE_CONTENT` privileged intent（< 100 servers 自開、之後審核） |
| Channel | 既有 `allowed_channels` 表 |
| User | Channel 內任何 user 都能設提醒（MVP 簡化） |
| Rate limit | 每 user 60 秒 1 次 LLM 呼叫，超過 silently drop |

### 5.4 Bot 自我識別

新增 `DISCORD_BOT_USER_ID` 環境變數，從 Developer Portal 抄出來、啟動時讀進 `Config`。比 Gateway READY event 取得更早可用。

## 6. `schedule_reminder` Tool

### 6.1 Schema（Claude API）

```json
{
  "name": "schedule_reminder",
  "description": "為使用者排一個未來時間點的單次提醒。當使用者明確要求被提醒某件事在某時刻時呼叫。如果時間表達模糊（例如「等等」、「待會」），不要呼叫此 tool，先用文字回問清楚。",
  "input_schema": {
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
  }
}
```

### 6.2 執行邏輯

```rust
pub async fn schedule_reminder(
    args: ScheduleReminderArgs,
    ctx: &ToolContext,         // user_id, guild_id, channel_id, source_message_id, db_pool
) -> ToolResult {
    if args.fire_at < Utc::now() - Duration::seconds(5) {
        return ToolResult::Error("fire_at 必須是未來時間".into());
    }
    if args.fire_at > Utc::now() + Duration::days(365) {
        return ToolResult::Error("提醒時間不能超過一年後".into());
    }
    if args.body.chars().count() > 500 {
        return ToolResult::Error("body 過長（>500 字）".into());
    }

    let id = db::reminders::insert(ctx.db_pool, &Reminder { /* ... */ }).await?;

    ToolResult::Success(json!({
        "reminder_id": id,
        "fire_at_local": args.fire_at.with_timezone(&Tz::Asia_Taipei).to_rfc3339(),
        "body": args.body
    }))
}
```

### 6.3 設計取捨

- **時間換算交給 LLM**：description 明寫「使用者通常給台灣時間，要先換 UTC」；配合既有 `get_current_time` tool，LLM 能算出絕對 UTC。
- **Tool 不直接回 channel**：成功後回 LLM 一個 JSON 確認，LLM 會自然產人話「好，我會在 18:30 提醒你 X」。實際 reply 由 listen pipeline 的步驟 9 統一處理。
- **`ToolContext` 擴充**：既有介面要新增 `guild_id` / `channel_id` / `source_message_id` 欄位；slash command 路徑這幾個欄位仍可從 interaction 補出來，行為一致。
- **LLM 模型沿用 `claude-haiku-4-5`**：tool use + 時間換算 Haiku 夠用，有 regression 再升。

## 7. Polling + 觸發送出

### 7.1 Polling job

於 `scheduler.rs` 加：

```rust
sched.add(Job::new_async("0/30 * * * * *", move |_uuid, _lock| {  // 每 30 秒
    let pool = pool.clone();
    let bot_token = bot_token.clone();
    Box::pin(async move {
        if let Err(e) = fire_due_reminders(&pool, &bot_token).await {
            tracing::error!("reminder fire batch failed: {e}");
        }
    })
})?).await?;
```

### 7.2 `fire_due_reminders`

```rust
async fn fire_due_reminders(pool: &PgPool, bot_token: &str) -> Result<()> {
    let due: Vec<Reminder> = sqlx::query_as(
        "SELECT * FROM reminders
         WHERE fired_at IS NULL
           AND fire_at <= now()
           AND failed_attempts < 5
         ORDER BY fire_at
         LIMIT 50
         FOR UPDATE SKIP LOCKED"
    ).fetch_all(pool).await?;

    for r in due {
        match send_reminder(bot_token, &r).await {
            Ok(()) => {
                sqlx::query("UPDATE reminders SET fired_at = now() WHERE id = $1")
                    .bind(r.id).execute(pool).await?;
            }
            Err(e) => {
                sqlx::query(
                    "UPDATE reminders
                     SET failed_attempts = failed_attempts + 1,
                         last_error = $2
                     WHERE id = $1"
                ).bind(r.id).bind(e.to_string()).execute(pool).await?;
            }
        }
    }
    Ok(())
}
```

### 7.3 REST 送出

```rust
async fn send_reminder(bot_token: &str, r: &Reminder) -> Result<()> {
    let body = json!({
        "content": r.body,
        "allowed_mentions": { "parse": [] },
        "message_reference": r.source_message_id.as_ref().map(|id| json!({
            "message_id": id,
            "fail_if_not_exists": false
        }))
    });

    let resp = REST_CLIENT
        .post(format!("https://discord.com/api/v10/channels/{}/messages", r.channel_id))
        .header("Authorization", format!("Bot {}", bot_token))
        .json(&body)
        .send().await?;

    if !resp.status().is_success() {
        bail!("HTTP {}: {}", resp.status(), resp.text().await.unwrap_or_default());
    }
    Ok(())
}
```

### 7.4 Edge cases

| 情境 | 處理 |
|---|---|
| Bot 重啟錯過觸發時間 | polling 啟動後 30 秒內補送（`fired_at IS NULL` 撈到） |
| 同筆連跑兩次 polling | `SKIP LOCKED` 擋掉，單 instance 也有 tokio 排程的 single-fire 語意 |
| Channel 被刪 / bot 被踢 | REST 404/403 → `failed_attempts++`，5 次後放棄、留 `last_error` |
| Rate limit (429) | 計入 `failed_attempts`，下輪 polling 再試。MVP 不做額外 backoff |
| Source message 被刪 | `fail_if_not_exists: false` 讓 reply 降級為一般訊息 |
| 超過 5 次失敗 | 留 `fired_at IS NULL` 但 query filter 掉，未來可手動處理或加 cleanup job |

## 8. Config、相依、部署

### 8.1 新環境變數

```
DISCORD_BOT_TOKEN=...              # Gateway 連線用（既有應已有，確認）
DISCORD_BOT_USER_ID=...            # 過濾自己的訊息
DISCORD_GATEWAY_ENABLED=true       # 預設 true；local dev / 多 instance 想關時用
```

### 8.2 Discord Developer Portal

- 打開 **Message Content Intent**（< 100 servers 自開、之後審核）

### 8.3 Cargo 相依

```toml
twilight-gateway = "0.16"
twilight-model = "0.16"
```

不用 `serenity`：太重且預設帶過多我們不要的功能；REST 維持用既有 `reqwest`。

### 8.4 main.rs 啟動

```rust
if config.discord_gateway_enabled {
    tokio::spawn(discord::gateway::run(config.clone(), pool.clone(), assistant.clone()));
}
```

Gateway task 內部使用 twilight 內建的 reconnection loop，我們只 log。

### 8.5 部署影響

- VM docker compose：確認 `restart: unless-stopped` 保留，讓 WebSocket 斷線後容器層也能恢復
- Cloudflare Tunnel：不影響（Gateway 是 outbound WebSocket）
- 不跟既有 interactions endpoint 衝突，兩條路徑同 process

## 9. 測試策略

| 層 | 範圍 | 工具 |
|---|---|---|
| Unit | `schedule_reminder` 時間 / 長度 validation | `#[tokio::test]` + 假 ToolContext |
| Unit | `fire_due_reminders` query 邏輯 | sqlx + testcontainer postgres（已有先例） |
| Integration | 插 fire_at = now()+1s → 等 2s → 驗 fired_at 非 NULL | testcontainer + mock Discord REST |
| Manual | dev guild 真實對話、觀察送出 | 跑 dev bot |

不做：Gateway listener 的 mock 測試 — twilight 沒給好的 mock，價值低。Listen pipeline 的核心邏輯（allowed_channels filter、LLM 空字串短路）抽出來獨立測。

## 10. 已知 follow-ups（MVP 後）

1. **取消 / 列表 / 重複提醒** — `cancel_reminder`、`list_reminders` tool；schema 加 `recurrence` 欄位
2. **跨 user 在同 channel 的對話脈絡** — conversation 目前用 `(user_id, channel_id)` 為鍵，listen 模式下 A 設提醒、B 的訊息看不到 A 的脈絡；要做須改 conversation schema
3. **多 instance 水平擴展** — `SKIP LOCKED` 已預埋；還需處理 Gateway shard 分配（twilight `Shard` 多 instance 模式）
4. **成本 hard cap** — 單 guild 每日 LLM 呼叫上限，避免被洗版打爆
5. **LINE 平台** — 表已有 `platform` 欄位；LINE 無 Gateway，要另寫 webhook → listen pipeline bridge
