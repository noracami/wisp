# TPP POC Stage 1 — 回來要處理的事

> 狀態時點：2026-04-23，HEAD `1a75d4d`
> 計畫檔：`docs/superpowers/plans/2026-04-23-webhook-interaction-bridge-poc-stage1.md`

## 目前進度

已完成 Task 1–8（合計 9 個 commit），程式碼部分全部就位：

| Task | Commit | 說明 |
|---|---|---|
| T1 | `f0aa8fe` | PocState scaffold |
| T2 | `cc7b901` | `handle_setup` |
| T3 | `08791f8` | `handle_ping`（未登記） |
| T4 | `c840255` | `handle_ping`（webhook POST + wiremock） |
| T5 | `b4fcb0c` | `handle_click` |
| T6 | `1fcdef9` | handler.rs → twilight-model enums |
| T6.1 | `aeefe2a` | plan doc 小修（`From` → `try_from`） |
| T7+T8 | `1a75d4d` | handler.rs + main.rs 接線，`DiscordState` 加 `poc` 欄位 |

TPP 相關測試全綠（`tpp_poc_test` 9/9、`interaction_test` 2/2、`discord_verify_test` 2/2、`discord_webhook_test` 2/2、`config_test` 2/2）。

---

## 你要決定的事（優先順序由上而下）

### 決定 1：T7+T8 commit 裡的 out-of-scope 修復

`1a75d4d` 除了 T7+T8 任務內容，**還順手修了一個 pre-existing 的語法錯誤** (`tests/config_test.rs` 的孤立 `};`)。commit message 有誠實記錄。

**選項：**
- **a.** 接受現況（commit 已 push，保持不動）
- **b.** 改回來、另開一個 commit 單獨記錄那個 pre-existing 修復
- **c.** 完全 revert `tests/config_test.rs` 那行變更，自己處理

**我的傾向**：a。變更本身是對的，動機也寫明了。但這是你的 codebase，你決定。

---

### 決定 2：repo 既有的兩個測試失敗（與 POC 無關）

實測確認：在 T7+T8 **之前的 commit（`aeefe2a`）** 就已經無法編譯。

- **`tests/assistant_test.rs`**：`Assistant::new` 舊簽章 4 參數，目前 `src/assistant/service.rs` 的 `new` 要 5 參數。test file 沒跟進。
- **`tests/llm_claude_test.rs`**：`LlmResponse::Text` / `ToolUse` 從 tuple variant 改成 struct variant（多了 `model`、`usage` 欄位）。test file 還用舊 pattern。

這兩個跟 POC 毫無關聯，屬 repo 既有 debt。

**選項：**
- **a.** 不處理，POC 完成再說（最低阻力）
- **b.** 順手修了（擴大 scope，但讓 repo 回到全綠）
- **c.** 新增 TODO 條目 `T-XXX`，安排之後單獨處理

**我的傾向**：c（開 TODO）——POC 的重點在實驗結果不在 repo 衛生，但也不要讓這個 debt 被忘記。

---

### 決定 3：T7+T8 尚未跑 spec review / code quality review

按 subagent-driven-development 流程，每個 task 要有兩層 review。T7+T8 還沒做。

**選項：**
- **a.** 回來後我直接派兩個 review subagent，你不用做什麼
- **b.** 跳過，理由是代碼已經 push、不想再迭代
- **c.** 你自己讀 `git show 1a75d4d` 來 review

**我的傾向**：a（自動跑 review，有問題再說）。

---

### 決定 4：T9（手動）— 你必須自己做的 Discord Dev Portal 設定

程式碼不能再幫你推進這步。你要：

1. **Discord Developer Portal**（https://discord.com/developers/applications）→ 選 Wisp app → Installation 分頁：
   - 確認 "User Install" 已啟用（保留 Guild Install 不影響現有功能）
   - 複製 Install Link
2. **用 Install Link 授權 Wisp app 安裝到你自己的 Discord 帳號**（不是任何 server）
3. **註冊兩個 user-installed slash commands**（用 `discord-commands` skill 或 curl）：

   **`tpp-setup`**：
   ```json
   POST /applications/{application_id}/commands
   Authorization: Bot <bot_token>

   {
     "name": "tpp-setup",
     "description": "Register a webhook URL for TPP POC",
     "integration_types": [1],
     "contexts": [0, 1, 2],
     "options": [
       {"type": 3, "name": "url", "description": "Discord webhook URL", "required": true}
     ]
   }
   ```

   **`tpp-ping`**：
   ```json
   {
     "name": "tpp-ping",
     "description": "Send a test button message to the registered webhook",
     "integration_types": [1],
     "contexts": [0, 1, 2]
   }
   ```

   `integration_types: [1]` 是關鍵 — 代表 user install，不需要 bot 裝在 guild。

4. 在 Discord 任何 channel 打 `/`，看 `tpp-setup` 和 `tpp-ping` 會不會出現在 autocomplete。

---

### 決定 5：T10（手動）— 跑三個實驗並記錄結果

**前提**：Wisp 要在 Discord 可連到的位置跑著（production VM / 本機 + ngrok 皆可）；Interactions Endpoint URL 已在 Dev Portal 設好。

**實驗 A（Guild channel，bot 未安裝）**
1. 到某個 Discord server 的 channel（你需要 Manage Webhooks 權限，bot 不需要），手動建 webhook、複製 URL
2. `/tpp-setup url:<webhook_url>` — 觀察 server log 的 `tpp_poc.setup` 事件，記錄整個 payload（有無 `channel_id` / `guild_id` / `context` / `authorizing_integration_owners`）
3. `/tpp-ping` — 觀察 `tpp_poc.ping.send.start/done`；確認按鈕訊息出現在頻道
4. **點按鈕** — 觀察 `tpp_poc.click` 事件是否出現（關鍵 Q3）；記錄 click payload（`application_id` / `message.webhook_id` / `channel_id` / `user.id` or `member.user.id` / `data.custom_id`）

**實驗 B（Bot DM）**
- 對 Wisp bot 的私訊執行 `/tpp-setup`（不跑 `/tpp-ping`），記錄 payload 差異

**實驗 C（Group DM）**
- 在 group DM 執行 `/tpp-setup`，記錄 payload 差異

**結果寫入**：
append 一個 `## POC 結果 (Stage 1)` 區塊到 `docs/feature-request/webhook-interaction-bridge-poc.md`（計畫 Step 10.5 有範本）。

**成功條件**（= 可以進階段 2）：
- ✅ Q3：`tpp_poc.click` 事件在點擊後有出現
- ✅ Q4：click payload 含足以識別「誰點的 + 哪則訊息」的欄位

**失敗分岔**：Q3 完全沒 route → 另寫 follow-up spec，改走 OAuth `webhook.incoming`。

---

### 決定 6：T10 完成後的 final code review

subagent-driven-development 流程最後一步。程式碼部分。

**選項：**
- **a.** T10 完成後回到對話，我自動派一個 code-reviewer 跑整個 POC range（`3b42a6e..HEAD`）
- **b.** 跳過，你覺得 per-task review 已經夠了

**我的傾向**：a。

---

## 重新接手對話時

就貼這份 memo 的路徑給我，我會從你選定的那個決定繼續推進。順序上建議先處理決定 1 + 2（短），再走 3（T7+T8 review），然後 4 / 5（你親自做）。
