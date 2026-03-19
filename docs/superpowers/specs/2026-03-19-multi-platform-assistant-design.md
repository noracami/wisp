# Wisp 多平台 AI 助理服務架構設計

## 背景

Wisp 原本是 Discord 專用的 AI 聊天機器人。本次架構調整將 Wisp 轉型為**平台無關的 AI 助理服務**，Discord 和 LINE 只是接入的通道，核心邏輯（對話管理、記憶、LLM 呼叫、Tool Use）完全不依賴任何平台。

## 架構：分層設計

```
Platform Layer    →  /discord/*, /line/*     （收發訊息、簽章驗證、格式轉換）
       ↓ ChatRequest
Core Layer        →  assistant service       （對話管理、記憶、呼叫 LLM + tool call loop）
       ↓ tool call
Tool Layer        →  weather, ...            （LLM 可呼叫的工具，以 trait 統一介面）
```

- **Platform Layer** 負責把各平台的訊息轉成統一的 `ChatRequest`，處理完後用平台 API 回覆。
- **Core Layer** 完全不知道訊息來自哪個平台。
- **Tool Layer** 以統一 trait 定義，LLM 透過 function calling 決定使用哪些。
- 未來遷移到 Event Bus 架構（方案 C）成本低：`ChatRequest` 本身即為 event payload，加一層 dispatch 即可，Core 和 Tool Layer 不需變動。

## 目錄結構

```
src/
├── main.rs
├── lib.rs
├── config.rs
├── error.rs
│
├── platform/                  # Platform Layer
│   ├── mod.rs                 # Platform enum, ChatRequest, ChatResponse, ReplyContext
│   ├── discord/
│   │   ├── mod.rs
│   │   ├── handler.rs         # Discord Interactions Endpoint handler
│   │   ├── verify.rs          # Ed25519 簽章驗證
│   │   └── webhook.rs         # Discord Webhook 客戶端
│   └── line/
│       ├── mod.rs
│       ├── handler.rs         # LINE Webhook handler + HMAC-SHA256 驗證
│       └── client.rs          # LINE Messaging API reply/push
│
├── core/                      # Core Layer
│   ├── mod.rs
│   └── assistant.rs           # ChatRequest → 記憶 → LLM → tool call loop → ChatResponse
│
├── llm/                       # LLM 客戶端
│   ├── mod.rs
│   └── claude.rs              # 擴充支援 tools 參數與 tool_use 回應解析
│
├── db/                        # 資料層
│   ├── mod.rs
│   ├── memory.rs              # 對話記憶（平台無關，用 user_id UUID）
│   └── users.rs               # 使用者身份 + 平台帳號管理
│
├── tools/                     # Tool Layer
│   ├── mod.rs                 # Tool trait, ToolRegistry
│   └── weather.rs             # 天氣預報 tool
│
├── weather/                   # 天氣資料來源（供 tool 和 scheduler 共用）
│   ├── mod.rs
│   └── cwa.rs
│
└── scheduler.rs               # 定時任務（直接呼叫 CwaClient，不經過 tool 系統）
```

## 統一訊息格式

```rust
pub enum Platform {
    Discord,
    Line,
}

/// Platform Layer 轉換後交給 Core 的統一請求（不含平台特定資訊）
pub struct ChatRequest {
    pub user_id: Uuid,              // 統一使用者 ID
    pub channel_id: String,         // Discord: channel ID; LINE: group/room ID，1-on-1 時為 user ID
    pub platform: Platform,         // 來源平台
    pub message: String,            // 使用者訊息內容
}

/// Core 回傳給 Platform handler 的回應（純內容，不含平台特定資訊）
pub struct ChatResponse {
    pub text: String,
}
```

## Core Layer 流程

```
ChatRequest 進來
  → 用 user_id + channel_id + platform 取得/建立 conversation
  → 儲存使用者訊息
  → 載入歷史訊息
  → 呼叫 LLM（帶 tools 定義）
  → 如果 LLM 回 tool_call → 執行 tool → 把 tool_result 餵回 LLM
  → 重複直到 LLM 回純文字（上限 10 輪，超過則回傳錯誤訊息）
  → 儲存助理回應（僅儲存最終的使用者訊息與助理文字回應，tool call 中間過程不入庫）
  → 回傳 ChatResponse
```

Core 回傳 `ChatResponse`（僅含文字），Platform handler 自行持有回覆所需的平台資訊（interaction token、reply token 等），用平台 API 把回應發出去。

## 資料庫 Schema

```sql
-- 統一使用者
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    display_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 平台身份映射（一個 user 可有多個平台帳號）
CREATE TABLE platform_identities (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id),
    platform TEXT NOT NULL,          -- 'discord', 'line'
    platform_user_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (platform, platform_user_id)
);

-- conversations 綁定統一 user_id
CREATE TABLE conversations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id),
    channel_id TEXT NOT NULL,
    platform TEXT NOT NULL,           -- 'discord', 'line'（與 Platform enum 對應）
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_conversations_user_channel
    ON conversations(user_id, channel_id, platform, updated_at DESC);

-- messages 不變
CREATE TABLE messages (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    conversation_id UUID NOT NULL REFERENCES conversations(id),
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content TEXT NOT NULL,
    embedding VECTOR(1024),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

**對話生命週期**：
- 同一個 `(user_id, channel_id, platform)` 可以有多個 conversation（不設 UNIQUE constraint）
- 每次收到訊息時更新 conversation 的 `updated_at`
- 查詢時取 `updated_at` 最近且在 30 分鐘內的 conversation，超過 30 分鐘則建立新的 conversation
- `load_recent_messages` 查詢使用 subquery：先 `ORDER BY created_at DESC LIMIT N`，再外層 `ORDER BY created_at ASC`，確保取到最新的 N 條並按時序排列

**使用者查詢流程**：
1. 平台收到訊息，用 `(platform, platform_user_id)` 查 `platform_identities`
2. 找到 → 取得 `user_id`；找不到 → 自動建立 `users` + `platform_identities`
3. 用 `user_id` + `channel_id` + `platform` 操作 `conversations`

**帳號綁定**：後台手動將兩個 `platform_identities` 指向同一個 `users.id`，記憶自動打通。未來可加指令綁定流程。

## Tool Use 架構

```rust
/// 需使用 async-trait 或手動 desugaring 以支援 dyn dispatch
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;     // JSON Schema
    async fn execute(&self, input: Value) -> Result<String, AppError>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}
```

- `ToolRegistry` 在啟動時建立，註冊所有可用 tools
- `tool_definitions()` 產出給 Claude API 的 tools 陣列
- `execute(name, input)` 根據 tool name 執行對應 tool
- 天氣預報為第一個 tool（`get_weather`），Scheduler 定時任務繼續直接呼叫 `CwaClient`，不經過 tool 系統

**Claude 客戶端變更**：
- `ClaudeClient::chat` 增加 `tools` 參數
- 回應解析處理 `tool_use` 類型的 content block
- Core 層負責 tool call loop

## 路由

| 路由 | 用途 |
|------|------|
| `GET /health` | 健康檢查 |
| `POST /discord/interactions` | Discord Interactions Endpoint |
| `POST /line/webhook` | LINE Webhook |

```rust
let app = Router::new()
    .route("/health", get(|| async { "ok" }))
    .nest("/discord", discord_routes(assistant.clone(), &config))
    .nest("/line", line_routes(assistant.clone(), &config));
```

## 啟動流程

```rust
#[tokio::main]
async fn main() {
    // 1. 載入設定、建立 DB pool、跑 migration
    // 2. 建立共用服務
    let memory = Arc::new(Memory::new(pool));
    let users = Arc::new(UserService::new(pool));
    let claude = Arc::new(ClaudeClient::with_default_url(&config.anthropic_api_key));
    let tool_registry = Arc::new(build_tool_registry(&config));
    let assistant = Arc::new(Assistant::new(claude, memory, users, tool_registry));

    // 3. 組路由（nest /discord, /line）
    // 4. 啟動 scheduler
    // 5. 啟動 server
}
```

## Platform Handler 職責

各平台 handler 的職責收斂為：

1. 驗證簽章（Discord: Ed25519, LINE: HMAC-SHA256）
2. 解析平台格式，組出 `ChatRequest`
3. 查/建使用者身份（透過 `UserService`）
4. 呼叫 `assistant.handle(chat_request)` 取得 `ChatResponse`
5. 用平台 API 回覆

Discord 額外處理 defer（先回 type 5，spawn 背景任務再更新回應）。LINE 先回 200，spawn 背景任務，用 reply token 回覆；若 reply token 過期（約 1 分鐘），改用 push message API 發送（需要使用者的 LINE user ID，已在 `platform_identities` 中）。

## 補充說明

### 資料遷移

專案處於早期開發階段，現有資料可捨棄。新 schema 以新的 migration 檔案提供，舊 `001_init.sql` 的 schema 將被取代。

### 平台設定為可選

`Config` 中各平台的設定（Discord、LINE）為可選。服務啟動時只註冊有設定的平台路由，未設定的平台不載入。這允許開發者只跑單一平台進行開發測試。

### Platform enum 與資料庫字串對應

| Rust enum | DB 字串 |
|-----------|---------|
| `Platform::Discord` | `'discord'` |
| `Platform::Line` | `'line'` |

### ChatMessage 統一

現有 `crate::llm::ChatMessage` 和 `crate::db::memory::ChatMessage` 重複定義，重構時統一為一個，放在 `core/` 或頂層模組。
