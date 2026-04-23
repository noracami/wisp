# Webhook-Interaction Bridge POC — Stage 2 Design

> 對應 feature request: [`docs/feature-request/webhook-interaction-bridge-poc.md`](../../feature-request/webhook-interaction-bridge-poc.md)
> 前置 spec: [`2026-04-23-webhook-interaction-bridge-poc-stage1-design.md`](./2026-04-23-webhook-interaction-bridge-poc-stage1-design.md)

## 背景與定位

### 為什麼有 Stage 2

Stage 1 做完三個實驗後，確認 **manually-created channel webhook 無法帶 `components`**（Discord 靜默丟棄按鈕）。Q2 在 API 層被阻擋，Q3/Q4 無法測。

Stage 2 改走 Discord 官方支援的 **OAuth2 `webhook.incoming` scope**：使用者一次授權 → Wisp 取得一個由 **application 擁有** 的 webhook → 該 webhook 允許帶 components → 終於能驗證 Q3/Q4。

### 要驗證的未知

Stage 1 的 Q1 已解。Stage 2 focus 在以下未知：

| # | 問題 | 觀察點 |
|---|---|---|
| Q2′ | Application-owned webhook 是否真能 render `components`？ | 授權後 POST 帶按鈕的訊息，看 channel 實際顯示 |
| Q3 | 按鈕 click 是否 route 到 `/discord/interactions`？ | 點按鈕後觀察 server log |
| Q4 | Click interaction payload 的 `application_id` / `message.webhook_id` / `channel_id` / `user.id` 長什麼樣？ | type:3 payload dump |
| Q6 | OAuth 授權流程 UX：一次授權能覆蓋多少範圍？`channel_id` 是使用者選還是 Wisp 指定？ | 實作授權頁並觀察 |
| Q7 | OAuth callback response 的 `webhook` object 欄位是否足夠直接執行？還需要額外 API 呼叫嗎？ | 收到 callback 時 dump 整個 token response |

### 明確不在 Stage 2 範圍

- 完整 TPP 遊戲迴圈（StoryEngine、GameRegistry、多輪結算）
- 同一 user 授權多個 channel 的管理 UX
- Webhook 撤銷 / 重綁流程
- `/tpp-ping` 的訊息自訂（固定單一按鈕即可）
- 錯誤訊息 UX 打磨

上述延後到 Stage 3 spec。

---

## 架構

### 整體流程

```
使用者打 /tpp-setup
        ↓
Wisp 回 ephemeral 訊息：「點此授權 → <OAuth URL>」
        ↓
使用者點連結（在瀏覽器開）
        ↓
Discord 顯示授權頁：選 server / channel，確認授權
        ↓
Discord 302 redirect 到 Wisp 的 /discord/oauth/callback?code=...&state=...
        ↓
Wisp 用 code 跟 Discord 換 access_token + webhook object
        ↓
存進 DB：user_id → (webhook_id, webhook_token, channel_id, guild_id)
        ↓
Wisp 回 HTML 成功頁「授權完成，回 Discord 打 /tpp-ping」
        ↓
使用者打 /tpp-ping
        ↓
Wisp 從 DB 查 webhook，POST 帶按鈕訊息
        ↓
使用者點按鈕 → Discord POST 到 Wisp 的 Interactions Endpoint
        ↓
handle_click 印 payload log（Q3 / Q4 驗證點）
```

### 檔案配置

```
src/tpp_poc.rs                       ← 擴充：加 handle_oauth_callback、改 handle_setup、handle_ping 改讀 DB
src/platform/discord/oauth.rs        ← 新增：OAuth URL 建構、token exchange、state 管理
src/platform/discord/handler.rs      ← 修改：加 /discord/oauth/callback route
src/config.rs                        ← 修改：DiscordConfig 加 oauth_redirect_uri、client_secret（已有 application_id）
migrations/00XX_tpp_webhooks.sql     ← 新增：tpp_webhooks 資料表
src/db/tpp_webhooks.rs               ← 新增：Webhook repository (CRUD 薄層)
```

**module 命名仍保留 `tpp_poc`**：Stage 2 還在 validation 階段，若 Q3/Q4 通過，Stage 3 才改名為 `tpp_bridge` 或類似正式名稱。

### 資料模型

```sql
CREATE TABLE tpp_webhooks (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      TEXT NOT NULL,              -- Discord user ID (從 OAuth state 帶進來)
    webhook_id   TEXT NOT NULL,              -- Discord webhook ID
    webhook_token TEXT NOT NULL,             -- Webhook execute token（敏感！）
    channel_id   TEXT NOT NULL,
    guild_id     TEXT,                        -- 可能是 null（DM 不會，但防守性）
    channel_name TEXT,                        -- 顯示用
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id)                          -- POC 階段：一個 user 一筆，重授權覆蓋
);
```

**為何 `UNIQUE (user_id)`**：POC 不需多筆。重跑 OAuth 覆蓋舊的（`ON CONFLICT (user_id) DO UPDATE`）。Stage 3 可能改成 `UNIQUE (user_id, channel_id)` 支援多 channel。

**敏感資料**：`webhook_token` 在 DB 明文存放。POC 可接受（內部 DB 不對外），Stage 3 前需評估 encryption at rest。

### OAuth State 管理

為防 CSRF，授權連結必須帶 `state` 參數，callback 時驗證。

POC 簡化方案：**state = HMAC(user_id || issued_at_unix, secret)**
- 使用者打 `/tpp-setup` 時，用該 user_id + 當下時間生 state
- callback 收到 state 時，decode 驗 HMAC、檢查時間 < 10 分鐘
- 無需 DB / Redis 存 state → 一次性、stateless

secret 來源：複用 `DISCORD_PUBLIC_KEY` 或新增 `TPP_STATE_SECRET`。**建議新增**，避免語義混淆。

### 依賴走向

```
Discord handler (slash) ──► tpp_poc::handle_setup ──► oauth::build_authorize_url
                                                            │
                                                            └─► state = HMAC(user_id || ts)

OAuth callback route ────► oauth::exchange_code ──► Discord /oauth2/token
                                    │
                                    └─► TppWebhookRepo::upsert

Discord handler (type:3) ──► tpp_poc::handle_click (same as Stage 1)

Discord handler (/tpp-ping) ──► tpp_poc::handle_ping ──► TppWebhookRepo::find_by_user
                                                              │
                                                              └─► reqwest POST webhook
```

---

## 實作細節

### Discord Dev Portal 設定（手動，對應 Task 9.x）

1. **OAuth2** tab → 加 redirect URI：`https://wisp.miao-bao.cc/discord/oauth/callback`
2. 複製 **Client Secret**（新值）→ 存進 production `.env` 的 `DISCORD_CLIENT_SECRET`（新變數）
3. 同時 production `.env` 加 `DISCORD_OAUTH_REDIRECT_URI=https://wisp.miao-bao.cc/discord/oauth/callback`

### `Config` 擴充

```rust
pub struct DiscordConfig {
    pub application_id: String,
    pub public_key: String,
    pub bot_token: String,
    // 新增
    pub client_secret: String,
    pub oauth_redirect_uri: String,
    pub state_secret: String,      // 或複用 public_key，見前述
}
```

### `oauth::build_authorize_url`

```rust
pub fn build_authorize_url(
    application_id: &str,
    redirect_uri: &str,
    state: &str,
) -> String {
    let mut url = Url::parse("https://discord.com/api/oauth2/authorize").unwrap();
    url.query_pairs_mut()
        .append_pair("client_id", application_id)
        .append_pair("response_type", "code")
        .append_pair("scope", "webhook.incoming")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state);
    url.to_string()
}
```

**只要 `webhook.incoming` 一個 scope** — Discord 會在授權頁自動讓使用者選 server + channel。不需要 `bot` scope。

### `oauth::exchange_code`

```rust
pub async fn exchange_code(
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<TokenResponse> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
    ];
    let resp = reqwest::Client::new()
        .post("https://discord.com/api/oauth2/token")
        .basic_auth(client_id, Some(client_secret))
        .form(&form)
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.json::<TokenResponse>().await?)
}

#[derive(Deserialize, Debug)]
pub struct TokenResponse {
    pub access_token: String,     // 我們不需要用，但 log 留著
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: Option<String>,
    pub scope: String,
    pub webhook: IncomingWebhook,  // 核心 payload
}

#[derive(Deserialize, Debug)]
pub struct IncomingWebhook {
    pub id: String,
    pub token: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub name: Option<String>,
    pub url: String,              // 可直接用，或我們自己組 id/token
}
```

**Discord doc**：https://discord.com/developers/docs/topics/oauth2#webhooks

### `handle_setup` 新行為

```rust
pub async fn handle_setup(
    state: &PocState,
    cfg: &DiscordConfig,
    interaction: &Value,
) -> InteractionResponse {
    let Some(user_id) = extract_user_id(interaction) else {
        return ephemeral("⚠️ 無法取得 user id");
    };

    let state_token = oauth::generate_state(&user_id, &cfg.state_secret);
    let url = oauth::build_authorize_url(
        &cfg.application_id,
        &cfg.oauth_redirect_uri,
        &state_token,
    );

    ephemeral(format!(
        "點此授權 Wisp 建立 webhook：{url}\n（連結 10 分鐘後失效）"
    ))
}
```

**不再有 `url` option**，slash command 註冊要對應更新（見實驗章節）。

### `/discord/oauth/callback` route

```rust
async fn oauth_callback(
    State(state): State<Arc<DiscordState>>,
    Query(params): Query<OAuthCallbackParams>,
) -> impl IntoResponse {
    // 1. 驗 state HMAC
    let user_id = match oauth::verify_state(&params.state, &state.config.state_secret) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid state").into_response(),
    };

    // 2. 跟 Discord 換 token + webhook
    let token_response = match oauth::exchange_code(
        &state.config.application_id,
        &state.config.client_secret,
        &params.code,
        &state.config.oauth_redirect_uri,
    ).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(event = "tpp_poc.oauth.exchange.error", error = %e);
            return (StatusCode::BAD_GATEWAY, "token exchange failed").into_response();
        }
    };

    tracing::info!(
        event = "tpp_poc.oauth.callback",
        user_id = %user_id,
        webhook_id = %token_response.webhook.id,
        channel_id = %token_response.webhook.channel_id,
        guild_id = ?token_response.webhook.guild_id,
    );

    // 3. 存 DB
    if let Err(e) = state.tpp_webhooks.upsert(&user_id, &token_response.webhook).await {
        tracing::error!(event = "tpp_poc.oauth.db.error", error = %e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response();
    }

    // 4. 回成功 HTML
    Html(r#"<h1>✅ 授權完成</h1><p>回 Discord 打 /tpp-ping 測試</p>"#).into_response()
}

#[derive(Deserialize)]
struct OAuthCallbackParams {
    code: String,
    state: String,
}
```

### `handle_ping` 改讀 DB

```rust
pub async fn handle_ping(
    repo: &TppWebhookRepo,
    interaction: &Value,
) -> InteractionResponse {
    let user_id = extract_user_id(interaction).unwrap_or_default();

    let webhook = match repo.find_by_user(&user_id).await {
        Ok(Some(w)) => w,
        Ok(None) => return ephemeral("⚠️ 尚未授權，請先 /tpp-setup"),
        Err(e) => {
            tracing::error!(event = "tpp_poc.ping.db.error", error = %e);
            return ephemeral("❌ DB 錯誤");
        }
    };

    let url = format!(
        "https://discord.com/api/webhooks/{}/{}",
        webhook.webhook_id, webhook.webhook_token
    );

    // 其餘邏輯同 Stage 1
    // ...
}
```

### `handle_click`（type:3）不變

Stage 1 的 implementation 繼續沿用 — 只是這次終於能被觸發。

### Slash command 重新註冊

`/tpp-setup` 的 schema 改了（移除 `url` option）。需要 PATCH 或 DELETE + POST。

POC 路徑：直接 DELETE 舊的、重建新的（一次性）。保留 `scripts/input-tpp-creds.sh`，新寫 `scripts/register-tpp-commands.sh`。

```json
{
  "name": "tpp-setup",
  "description": "Authorize Wisp to post to a Discord channel via webhook",
  "integration_types": [1],
  "contexts": [0, 1, 2]
}
```

**options 移除**。

---

## 觀察與實驗

### Log 事件

| 事件 tag | 時機 | 回答 |
|---|---|---|
| `tpp_poc.setup` | `/tpp-setup` 收到 | （如 Stage 1，繼續觀察 payload 差異） |
| `tpp_poc.oauth.callback` | OAuth 回來 | Q6, Q7（webhook object 欄位） |
| `tpp_poc.oauth.exchange.error` | Token exchange 失敗 | 錯誤診斷 |
| `tpp_poc.ping.send.start/done` | POST webhook | Q2′（status + body）|
| `tpp_poc.click` | type:3 進來 | **Q3, Q4（這次會觸發！）** |

### 實驗

**Experiment A — 完整 round-trip（Guild channel）**

1. `/tpp-setup` → 取得授權連結
2. 瀏覽器開連結，授權到一個 guild 的 channel
3. 確認 OAuth callback 成功（瀏覽器看到成功頁 + server log `tpp_poc.oauth.callback`）
4. 檢查 DB：`SELECT * FROM tpp_webhooks` 應有一筆
5. `/tpp-ping` → 看 `tpp_poc.ping.send.done` status = 200 / 204
6. **確認 channel 訊息底下出現按鈕** ✅（這是 Q2′ 通過條件）
7. **點按鈕** → 看 log `tpp_poc.click` **是否出現**（Q3）+ 記錄 payload 欄位（Q4）

**Experiment B —（保留選項）授權到 DM / group DM**

Discord OAuth `webhook.incoming` 的授權頁**只能選 guild channel**。DM 無法建 app-owned webhook。→ 這條實驗路徑**預期不可行**，跳過。

### 成功條件

**必要通過（= Stage 3 可開工）**：
- ✅ Q2′：app-owned webhook 送的訊息**真的有按鈕**在 channel render
- ✅ Q3：按鈕 click 後 `tpp_poc.click` 事件**出現在 log**
- ✅ Q4：click payload 有 `user.id`（或 `member.user.id`）+ 某種 message 識別（`message.webhook_id` 或 `message.id`）

**不通過分岔**：
- Q2′ 失敗（app-owned webhook 也不 render components）→ **徹底收場**，寫 post-mortem，長程計畫需改成「要求 Wisp 當 bot 裝在 target guild」
- Q3 失敗（click 不 route）→ 深度 debug：確認 Interactions Endpoint URL、確認 `application_id` match、考慮是否需要 bot installation as fallback

### 紀錄產物

Stage 2 結束後，**append 一個 `## Stage 2 結果` 區塊**到 `docs/feature-request/webhook-interaction-bridge-poc.md`，內容：
- Q2′ / Q3 / Q4 / Q6 / Q7 的實際答案
- click payload 欄位完整列表（for Stage 3 registry 設計）
- Stage 3 的下一步決策

---

## 測試

### 單元測試

- `oauth::generate_state` + `oauth::verify_state` round-trip + expired state + tampered state
- `oauth::build_authorize_url` 產生的 URL query params 正確
- `handle_setup` 回 ephemeral 訊息含 authorize URL
- `handle_ping`：無 DB 記錄時回「尚未授權」；有記錄時呼叫 POST webhook（mock）
- `TppWebhookRepo::upsert` → `find_by_user` round-trip

### 整合測試

- **不**做 real Discord OAuth flow 的 e2e 測試（手動實驗即可）
- `/discord/oauth/callback` route 的 happy path 用 mock Discord token endpoint（wiremock）

### 不涵蓋

- Signature verification（既有測試足夠）
- 真實 Discord API 行為（手動實驗）

---

## 風險與緩解

| 風險 | 可能性 | 緩解 |
|---|---|---|
| Q2′ 也失敗（components 還是不 render） | 低 | 已有 post-mortem 路徑；屬 Discord 平台限制，Wisp 無能為力 |
| OAuth state secret 洩漏 | 中 | 用獨立 `TPP_STATE_SECRET`，定期輪換 |
| Webhook token 明文存 DB | 高（資安） | POC 接受；Stage 3 前評估 encryption at rest 或 KMS |
| `channel_id` 使用者選錯 guild / 選 private channel | 中 | 成功頁提示「請確認選到預期頻道」；Stage 3 要有 list/unbind 功能 |
| OAuth callback 被 DoS（亂送 code） | 低 | Discord 回 400 即可，不影響其他功能 |
| 多使用者同時授權（race） | 低 | `UNIQUE (user_id)` + `ON CONFLICT UPDATE` 自然處理 |

---

## 驗收清單

Stage 2 算完成當且僅當：

- [ ] Discord Dev Portal 設定 OAuth2 redirect URI + 取得 client secret
- [ ] `config.rs` 擴充 `DiscordConfig` 三個新欄位
- [ ] `migrations/00XX_tpp_webhooks.sql` 寫好並過 migration
- [ ] `src/db/tpp_webhooks.rs` repository 實作 + 單元測試
- [ ] `src/platform/discord/oauth.rs` 實作 state HMAC + authorize URL + token exchange + 單元測試
- [ ] `src/tpp_poc.rs` 的 `handle_setup` 改成回授權連結
- [ ] `src/tpp_poc.rs` 的 `handle_ping` 改成從 DB 讀 webhook
- [ ] `src/platform/discord/handler.rs` 加 `/discord/oauth/callback` route
- [ ] `handle_click` 繼續沿用 Stage 1 實作（無改動）
- [ ] Slash command `/tpp-setup` 重新註冊（移除 `url` option）
- [ ] Experiment A 跑完，Q2′ + Q3 + Q4 有明確結果
- [ ] 結果 append 到 `docs/feature-request/webhook-interaction-bridge-poc.md`
