# Webhook-Interaction Bridge POC — Stage 1 Design

> 對應 feature request: [`docs/feature-request/webhook-interaction-bridge-poc.md`](../../feature-request/webhook-interaction-bridge-poc.md)

## 背景與定位

### 為什麼做這個 POC

Wisp 想探索一種新的 Discord 整合模式：**bot 不安裝在 guild**，純粹靠「**webhook 送出訊息 + Interactions Endpoint 收 button click**」達成互動。長程目標是 Twitch Plays Pokemon 風格的多輪投票遊戲（bot 發訊息 → 使用者投票 → 時間到 bot 根據結果發下一則），主題未定。

但在投入完整遊戲迴圈之前，有**若干關鍵未知**尚未驗證。這份 spec 是**階段 1**，專門做 empirical validation。階段 2（完整 TPP 遊戲迴圈）等階段 1 拿到實驗結果後再另寫 spec。

### 要驗證的未知

| # | 問題 | 觀察點 |
|---|---|---|
| Q1 | User-installed slash command 的 interaction payload 長什麼樣？有 `channel_id` / `guild_id` 嗎？ | `/tpp-setup` 觸發時 dump 整個 payload |
| Q2 | 手動建的 webhook 能不能接受 `components` 欄位？會不會被 Discord 拒絕？ | `/tpp-ping` 時觀察 HTTP response |
| Q3 | 手動 webhook 的 button click 會不會 route 到我們的 Interactions Endpoint？ | 點按鈕後觀察 server log |
| Q4 | 若 Q3 通過，click interaction payload 的 `application_id` / `message.webhook_id` / `channel_id` / `user.id` 長什麼樣？ | type:3 payload dump |
| Q5 | Bot 沒裝在 guild 時，user-installed 指令在不同 context（guild channel / DM / group DM）行為是否一致？ | 手動在三種 context 各跑一次 |

### 明確不在階段 1 範圍

- StoryEngine trait、故事樹、4 選項
- GameRegistry、per-channel 鎖、計時結算、多輪迴圈
- `allowed_channels` 公開性檢查
- DELETE webhook 清理
- 錯誤訊息的 UX 打磨

上述全部延後到階段 2 spec。

---

## 架構

### 檔案配置

```
src/tpp_poc.rs                    ← 新增，單檔搞定整個 POC（~150-250 行）
src/platform/discord/handler.rs   ← 修改：加 type:3 分支、slash command 名稱分派
src/lib.rs                        ← 修改：pub mod tpp_poc
src/main.rs                       ← 修改：PocState 注入 DiscordState
```

**為什麼單檔**：階段 1 全部邏輯約 150-200 行。分成多檔反而讓讀者跳檔。等階段 2 擴充成完整 TPP 迴圈時再拆模組。

### 依賴走向

```
Discord handler  ──► tpp_poc::handle_setup / handle_ping / handle_click
                           │
                           ├─► PocState (in-memory HashMap)
                           └─► reqwest (POST to webhook URL)
```

TPP POC 邏輯不呼叫任何 Discord bot API — 送訊息走 webhook URL，回應 interaction 直接用 twilight-model 結構體 JSON-serialize。

### `PocState`

```rust
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct PocState {
    pub webhooks: RwLock<HashMap<String /*user_id*/, String /*webhook_url*/>>,
}
```

**Registry key 用 `user_id`**：user-installed slash command 的 payload 必含 `user.id`（或 `member.user.id`），而 `channel_id` 是否存在正是 Q1 要驗證的東西，不能提前當 key。一個使用者同時只綁一個 webhook，對 POC 夠用。

---

## 實作細節

### Discord consts：改用 twilight-model enums

既有 `handler.rs` 目前用裸 u64 比對 interaction type。這次順手遷移到 `twilight-model` 的 enum：

**Receive 端（dispatch）**：

```rust
use twilight_model::application::interaction::InteractionType;

let kind = interaction["type"]
    .as_u64()
    .map(|n| InteractionType::from(n as u8));

match kind {
    Some(InteractionType::Ping) => ...,
    Some(InteractionType::ApplicationCommand) => {
        let command_name = interaction["data"]["name"].as_str().unwrap_or("");
        match command_name {
            "tpp-setup" => tpp_poc::handle_setup(&state.poc, &interaction).await,
            "tpp-ping"  => tpp_poc::handle_ping(&state.poc, &interaction).await,
            _ => { /* 既有 Assistant 流程 */ }
        }
    }
    Some(InteractionType::MessageComponent) => {
        tpp_poc::handle_click(&interaction).await
    }
    _ => StatusCode::BAD_REQUEST.into_response(),
}
```

**Response 端**：

```rust
use twilight_model::http::interaction::{
    InteractionResponse, InteractionResponseData, InteractionResponseType,
};
use twilight_model::channel::message::MessageFlags;

// Ping → Pong
Json(InteractionResponse { kind: InteractionResponseType::Pong, data: None })

// Button click ACK（不更新訊息）
Json(InteractionResponse { kind: InteractionResponseType::DeferredUpdateMessage, data: None })

// Ephemeral slash command 回應
Json(InteractionResponse {
    kind: InteractionResponseType::ChannelMessageWithSource,
    data: Some(InteractionResponseData {
        content: Some("Registered".to_string()),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    }),
})
```

**不新增自訂 `consts.rs`** — 避免兩套命名系統並存。既有 handler.rs 內的裸數字順手一併替換。

### `/tpp-setup` 處理

1. 從 payload 擷取 `user.id`（或 `member.user.id`）— 同時印出整個 interaction payload 到 log 供觀察 Q1
2. 從 `data.options` 找 name="url" 的 string 值
3. `state.poc.webhooks.write().await.insert(user_id, url)`
4. 回 ephemeral `ChannelMessageWithSource`：`✅ Webhook registered`

### `/tpp-ping` 處理

1. 擷取 `user.id`
2. `state.poc.webhooks.read().await.get(&user_id).cloned()`
3. 無 → ephemeral `尚未登記 webhook，請先 /tpp-setup url:<url>`
4. 有 → POST 到 webhook URL（見下），記下 response status/body 到 log 供觀察 Q2；回 ephemeral `Sent`

### Webhook POST payload

用 twilight-model 的 `Component` 型別建構，`json!` 嵌入：

```rust
use twilight_model::channel::message::component::{
    ActionRow, Button, ButtonStyle, Component,
};

let button = Component::Button(Button {
    custom_id: Some("tpp-poc-test".to_string()),
    label: Some("Click me".to_string()),
    style: ButtonStyle::Primary,
    disabled: false,
    emoji: None,
    url: None,
    sku_id: None,
});
let row = Component::ActionRow(ActionRow { components: vec![button] });

let body = json!({
    "content": "POC button test — 請點下方按鈕",
    "components": [row],
});

let response = reqwest::Client::new()
    .post(webhook_url)
    .json(&body)
    .send()
    .await?;

tracing::info!(
    event = "tpp_poc.ping.send",
    status = response.status().as_u16(),
    body = %response.text().await.unwrap_or_default(),
);
```

### `handle_click`（type:3 分支）

1. 印出完整 interaction payload（`serde_json::to_string_pretty`）
2. 回 `Json(InteractionResponse { kind: InteractionResponseType::DeferredUpdateMessage, data: None })`

這個 handler 的唯一目的是「驗證收得到 + 記錄 payload」，不做任何業務邏輯。

### Slash command 註冊

**不寫在 Wisp 程式碼**。一次性透過 Discord REST API 註冊（用專案的 `discord-commands` skill 或等效 curl）：

```jsonc
POST /applications/{application_id}/commands
{
  "name": "tpp-setup",
  "description": "Register a webhook URL for TPP POC",
  "integration_types": [1],        // user install
  "contexts": [0, 1, 2],           // guild, bot DM, group DM
  "options": [{
    "type": 3,
    "name": "url",
    "description": "Discord webhook URL",
    "required": true
  }]
}

POST /applications/{application_id}/commands
{
  "name": "tpp-ping",
  "description": "Send a test button message to registered webhook",
  "integration_types": [1],
  "contexts": [0, 1, 2]
}
```

`integration_types: [1]` 是關鍵 — 這讓 app 可以 user-install，不需要 bot 裝在 guild。

### `DiscordState` 擴充

```rust
pub struct DiscordState {
    // 現有欄位...
    pub poc: Arc<PocState>,   // 新增
}
```

`main.rs` 建構時 `poc: PocState::new()`。

---

## 觀察與實驗

### Log 格式

三個關鍵事件：

| 事件 tag | 時機 | 回答 |
|---|---|---|
| `tpp_poc.setup` | `/tpp-setup` 收到 | Q1（整個 interaction payload） |
| `tpp_poc.ping.send` | POST webhook 前後 | Q2（status + body） |
| `tpp_poc.click` | type:3 進來 | Q3, Q4（整個 interaction payload） |

### 實驗

**Experiment A — Guild channel（bot 未裝）**
1. 在 Discord server 的 channel 手動建 webhook（頻道設定 → 整合 → Webhook → 新增）
2. 把 Wisp app 以 user-install 安裝到自己帳號
3. 在該 channel 打 `/tpp-setup url:<webhook_url>` — 看 log 回答 Q1
4. 打 `/tpp-ping` — 看 log 回答 Q2；確認 channel 出現按鈕訊息
5. 點按鈕 — 看 log 回答 Q3, Q4

**Experiment B — Bot DM**（對 Wisp bot 的私訊）
- 跑 `/tpp-setup`（不跑 `/tpp-ping`，bot DM 無法建 webhook）觀察 Q5

**Experiment C — Group DM**
- 同 B

### 成功條件

**必要通過（= 階段 2 可開工）**：
- ✅ Q3：點按鈕後 `tpp_poc.click` 事件有出現在 log
- ✅ Q4：click payload 含足以識別「誰點的 + 哪則訊息」的欄位（至少 `user.id` / `member.user.id` 加某種 message 識別）

**不通過分岔**：
- Q3 失敗（click 從不抵達）→ 另寫 follow-up spec，改走 OAuth `webhook.incoming` scope 取得 application-owned webhook
- Q3 通過但 Q4 缺 `channel_id` → 不致命；階段 2 registry 本來就用 `user_id` 當 key

**Nice-to-have**：
- Q1 / Q5 影響階段 2 設計但不影響階段 1 的 go/no-go

### 紀錄產物

實驗結束後，**append 一個 `## POC 結果` 區塊**到 `docs/feature-request/webhook-interaction-bridge-poc.md`，內容：
- 每個 Q 的實際答案
- Discord 回應的關鍵 payload 片段
- 階段 2 的下一步決策（直接做 / OAuth 補丁 / 放棄）

---

## 測試

### 單元測試

涵蓋：

- `handle_setup`：給模擬 interaction payload、驗 `webhooks` HashMap 被更新、回應是 ephemeral `ChannelMessageWithSource`
- `handle_ping`：
  - 未登記時回 ephemeral 錯誤
  - 已登記時會呼叫 HTTP POST（mock `reqwest` 或用本地 `httpmock`）
- `handle_click`：驗回 `DeferredUpdateMessage`
- `InteractionType` 分派：type:3 會走到 `handle_click`

### 不涵蓋

- Signature verification（既有測試足夠）
- Discord API 的真實行為（這是手動實驗的工作，不是自動化測試）

---

## 風險與緩解

| 風險 | 可能性 | 緩解 |
|---|---|---|
| Q3 失敗：click 完全不 route | 中 | 有明確 fallback 路徑（OAuth webhook.incoming） |
| user-install 在某 context 不會觸發 interaction | 低 | Experiment B / C 明確覆蓋 |
| Wisp app 現況不是 user-installable | 高 | Dev Portal 改設定即可，不影響現有 bot 功能 |
| webhook POST 的 rate limit | 低 | POC 流量極小 |

---

## 驗收清單

階段 1 算完成當且僅當：

- [ ] `src/tpp_poc.rs` 實作 setup / ping / click 三個 handler
- [ ] `handler.rs` 加入 type:3 分派與 slash command 名稱分派
- [ ] `handler.rs` 既有裸 u64 替換為 twilight-model enum
- [ ] 單元測試 covers 三個 handler 的主要路徑
- [ ] Wisp app 在 Discord Dev Portal 設為 user-installable
- [ ] `tpp-setup` / `tpp-ping` 兩個 slash command 已註冊
- [ ] Experiment A 跑完，Q3 + Q4 有明確結果
- [ ] Experiment B / C 跑完（除非 A 已足以回答全部問題）
- [ ] 結果 append 到 `docs/feature-request/webhook-interaction-bridge-poc.md`
