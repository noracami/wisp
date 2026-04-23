## 🛠️ Handoff: Wisp Webhook-Interaction Bridge POC

### 1. 核心驗證目標
證明可以透過 **Webhook URL** 送出按鈕，並透過 **Interactions Endpoint** 接收並解析點擊事件（不需要長連接 Gateway）。

### 2. 給 Code Agent 的實作清單

#### A. 基礎 Web Server (Axum)
建立一個 `POST /interactions` 路由，並處理以下兩種邏輯：
1.  **Ping 驗證**：若收到 `{"type": 1}`，回傳 `{"type": 1}`。
2.  **按鈕點擊解析**：若收到 `{"type": 3}` (Message Component)，在 Terminal 印出 `custom_id` 內容。

#### B. 簽名驗證（這步不可省略，否則 Discord 不會發送資料）
實作一個 `verify_discord_signature` 函數，使用 `ed25519-dalek` 驗證：
* **Input**: `X-Signature-Ed25519` (header), `X-Signature-Timestamp` (header), `request_body` (bytes).
* **Public Key**: 來自你的 Discord Developer Portal。

#### C. Webhook 發送腳本 (測試用)
撰寫一個簡單的 `curl` 指令或短腳本，直接對你的 **Webhook URL** 發送訊息，內容必須包含 `components` (Button)，且 `custom_id` 設為 `test_button_click`。

---

### 3. 測試驗證步驟
1.  **啟動 Rust Server** (監聽 8080)。
2.  **開啟隧道** (如 `ngrok http 8080`)。
3.  **設定 Discord Portal**：將 ngrok 的網址填入 `Interactions Endpoint URL` 並儲存成功（這代表 Ping 驗證已過）。
4.  **執行 Webhook 腳本**：讓頻道出現帶按鈕的訊息。
5.  **點擊按鈕**：觀察 Rust Server 是否成功印出 `test_button_click`。

---

### 4. 關鍵 Rust 代碼邏輯 (Code Agent 參考)

```rust
// 這是 Code Agent 需要實現的關鍵驗證邏輯片段
async fn interactions_handler(
    headers: HeaderMap,
    body: String, // 取得原始 Body
) -> impl IntoResponse {
    // 1. 驗證 Signature (這部分請 Agent 實作 ed25519 邏輯)
    if !verify_signature(&headers, &body, PUBLIC_KEY) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let interaction: Interaction = serde_json::from_str(&body).unwrap();
    match interaction.kind {
        InteractionType::Ping => Json(json!({ "type": 1 })).into_response(),
        InteractionType::MessageComponent => {
            println!("成功接收互動！Custom ID: {:?}", interaction.data.custom_id);
            // 回傳 6 代表「我收到了，但不更新訊息內容」
            Json(json!({ "type": 6 })).into_response()
        },
        _ => StatusCode::OK.into_response(),
    }
}
```

---

### 為什麼這樣做？
這能直接驗證你的 **Wisp** 能否維持「平時不連線（靜態 Webhook 推播）」但「需要時能接收決策（HTTP 回調）」的架構。

---

## POC 結果 (Stage 1)

> 實驗日期：2026-04-23｜HEAD：`c1d14fb`

### Q1 — User-installed slash command interaction payload 結構

✅ **有答案**。在 guild channel（context 0）觸發 `/tpp-setup` 時，payload 含：

| 欄位 | 值 | 說明 |
|---|---|---|
| `context` | `0` | Guild channel |
| `channel_id` | 有 | ✅ |
| `guild_id` | 有 | ✅ |
| `member.user.id` | 有 | 呼叫者 user ID |
| `authorizing_integration_owners` | `{"1": "<user_id>"}` | key `"1"` = user install，值為授權安裝的 user ID |

`authorizing_integration_owners.1` 是 user-installed app 的特徵欄位，可用於 stage 2 的 per-user authorization。

### Q2 — 手動建的 webhook 能否接受 `components`？

❌ **不行**。

- Discord API 回傳 **204**（接受，無 content）— 不報錯
- 但訊息在 channel 實際顯示時**按鈕被靜默丟棄**，只有 `content` 文字出現
- 根本原因：channel settings 手動建的 webhook，owner 是 user 而非 application，Discord 只允許 application-owned webhook 送帶 components 的訊息

### Q3 — 按鈕 click 是否 route 到 Interactions Endpoint？

⛔ **無法測試（被 Q2 阻斷）**。按鈕從未 render，無法點擊。

### Q4 — Click interaction payload 欄位

⛔ **無法取得（被 Q3 阻斷）**。

### Q5 — 不同 context（guild / Bot DM / Group DM）行為差異

⏭️ **跳過**。Q2 已明確失敗，Q5 為 nice-to-have，不影響 go/no-go 決策，不繼續測。

---

### 結論

**Stage 1 結論：此路不通。**

手動建 webhook 的路徑在 Q2 即被 Discord API 阻擋 — components 永遠被 silent strip，無法送出帶按鈕的訊息，整個 webhook-interaction round-trip 假設在此失效。

**下一步（Stage 2 方向）：改走 OAuth `webhook.incoming` scope**。

此方案讓 Wisp 取得 application-owned webhook（由 Discord OAuth 流程授予），此類 webhook 允許帶 `components`。使用者只需一次 OAuth 授權，Wisp 即可在對方頻道送出可互動的按鈕訊息，後續 click 仍走現有的 Interactions Endpoint。詳細設計另寫 Stage 2 spec。
