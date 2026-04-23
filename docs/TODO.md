# TODO

## 待實作

- [ ] T-001 **Discord guild 暱稱** — 新增 `discord_user_profiles` 表，存 per guild 的 nickname（`user_id`, `guild_id`, `nickname`, `joined_at`）。LINE 暱稱直接放 `platform_identities.display_name`。
- [ ] T-002 **`platform_identities` 加 `display_name` 欄位** — 存各平台的全域暱稱
- [ ] T-003 **Web 控制面板** — 管理設定、查看對話記錄、環境變數等
- [ ] T-004 **修復 `tests/assistant_test.rs`** — `Assistant::new` 已改為 5 參數（增 `db_pool`），test 仍用舊 4 參數簽章，無法編譯。與 TPP POC 無關，屬既有 debt。
- [ ] T-005 **修復 `tests/llm_claude_test.rs`** — `LlmResponse::Text` / `ToolUse` 已從 tuple variant 改為 struct variant（加 `model`、`usage` 欄位），test 還用舊 pattern，無法編譯。與 TPP POC 無關，屬既有 debt。
- [ ] T-006 **Revoke Stage 2 POC webhook** — 2026-04-24 在 bot-playground forum thread (`1486080800058245221`) 建的手動 webhook `Captain Hook 1` (id `1496928843196268725`) 驗證 component 被 strip 時用過。Discord channel 設定 → 整合 → Webhooks → 刪除。
- [ ] T-007 **修正 Stage 2 spec 的權限敘述錯誤** — `docs/superpowers/specs/2026-04-23-webhook-interaction-bridge-poc-stage2-design.md` 寫「授權者需要 Manage Webhooks」是錯的，2026-04-24 實測授權頁明寫要 **Manage Server (MANAGE_GUILD)**。Q8a/b/c 的梯度也要改寫（Manage Server 無 channel-level override，Q8b 實際不存在）。
- [ ] T-008 **寫 Stage 2 實驗結果** — append `## Stage 2 結果` 區塊到 `docs/feature-request/webhook-interaction-bridge-poc.md`：
  - Q2′ ✅ app-owned webhook render components
  - Q3 ✅ click route 到 interactions endpoint（log `tpp_poc.click`）
  - Q4 ✅ payload 含 `member.user.id` + `message.id` + `message.webhook_id`
  - Q6/Q7/Q9 記錄
  - Q8 三梯度改 Manage Server 版本（見 T-007）
  - 結論：Activity 方向比 webhook 互動更適合多人場景；app-owned webhook 仍可保留作單向推訊息用途（見 T-009）
- [ ] T-009 **新 feature request：單向 push 訊息（戰報）** — 使用者把 context 丟給 Wisp，Wisp 用 webhook 送 embed 訊息到指定 channel。一次性授權、無互動、無 Manage Server 門檻（手動 webhook 足矣）。POC 的 Stage 2 code（OAuth callback + `tpp_webhooks` table + `handle_ping` 骨架）大部分可直接復用，只需換掉固定的 button payload 為使用者提供的 embed 內容。寫到 `docs/feature-request/one-way-push.md`（或類似命名）。
