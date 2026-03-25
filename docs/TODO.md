# TODO

## 待實作

- [ ] T-001 **Discord guild 暱稱** — 新增 `discord_user_profiles` 表，存 per guild 的 nickname（`user_id`, `guild_id`, `nickname`, `joined_at`）。LINE 暱稱直接放 `platform_identities.display_name`。
- [ ] T-002 **`platform_identities` 加 `display_name` 欄位** — 存各平台的全域暱稱
- [ ] T-003 **Web 控制面板** — 管理設定、查看對話記錄、環境變數等
- [ ] T-004 **查詢時間工具** — 讓 LLM 能取得當前日期時間，回答時間相關問題
