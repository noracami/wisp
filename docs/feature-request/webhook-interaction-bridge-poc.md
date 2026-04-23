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
