# TODO

## 待實作

- [ ] T-001 **Discord guild 暱稱** — 新增 `discord_user_profiles` 表，存 per guild 的 nickname（`user_id`, `guild_id`, `nickname`, `joined_at`）。LINE 暱稱直接放 `platform_identities.display_name`。
- [ ] T-002 **`platform_identities` 加 `display_name` 欄位** — 存各平台的全域暱稱
- [ ] T-003 **Web 控制面板** — 管理設定、查看對話記錄、環境變數等
- [ ] T-004 **修復 `tests/assistant_test.rs`** — `Assistant::new` 已改為 5 參數（增 `db_pool`），test 仍用舊 4 參數簽章，無法編譯。與 TPP POC 無關，屬既有 debt。
- [ ] T-005 **修復 `tests/llm_claude_test.rs`** — `LlmResponse::Text` / `ToolUse` 已從 tuple variant 改為 struct variant（加 `model`、`usage` 欄位），test 還用舊 pattern，無法編譯。與 TPP POC 無關，屬既有 debt。
