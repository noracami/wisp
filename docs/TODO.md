# TODO

## 待實作

- [ ] **Discord guild 暱稱** — 新增 `discord_user_profiles` 表，存 per guild 的 nickname（`user_id`, `guild_id`, `nickname`, `joined_at`）。LINE 暱稱直接放 `platform_identities.display_name`。
- [ ] **Discord 公開頻道白名單** — DB 表記錄哪些 (guild_id, channel_id) 預設公開回覆，其餘 ephemeral
- [ ] **全域 Slash Command** — 目前是 Guild scope，測試穩定後改成 Global
- [ ] **User Install** — 設定 Discord App 支援 User Install，到處都能用
- [ ] **`platform_identities` 加 `display_name` 欄位** — 存各平台的全域暱稱
