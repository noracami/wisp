## 🛠️ Feature Request: Wisp Web Control Panel

> 對應 TODO: T-003

### 1. 目標
提供一個 Web 介面來管理 Wisp 的運行狀態、設定與歷史資料，避免每件事都要 SSH 進 VM 或直接改環境變數。

### 2. 預期功能範圍（初版草稿，待收斂）

#### A. 設定管理
- 檢視與編輯環境變數（`.env` 內容）
- 啟用 / 停用個別 platform（Discord / LINE）
- 調整 allowed_channels 白名單
- 切換使用的 Claude 模型、調整 system prompt

#### B. 對話記錄檢視
- 依使用者 / 頻道 / 時間篩選對話
- 單筆對話展開看 tool call 記錄、token 用量
- 匯出（CSV / JSON）

#### C. 運行狀態
- Token 用量統計（依模型 / 依使用者 / 依時間區段）
- 最近錯誤日誌摘要
- 服務健康狀態（DB 連線、scheduler 狀態）

#### D. 使用者管理
- `platform_identities` 檢視（同一使用者在 Discord / LINE 的 mapping）
- 暱稱覆寫
- 封鎖 / 解鎖使用者

### 3. 未決議題
- **驗證方式**：Google OAuth？單一 admin password？Cloudflare Access？
- **部署方式**：與 Wisp main binary 同 process、獨立 Rust binary、或 Next.js 前端 + Wisp 開 admin API？
- **前端技術**：Server-rendered（Askama / Maud）還是 SPA（Next.js / SvelteKit）？
- **權限分層**：只有 admin，還是 admin / viewer 兩種角色？
- **操作稽核**：設定變更是否需要留紀錄？

### 4. 非目標（暫不處理）
- 公開註冊 / 多租戶
- 即時對話介入（由 web 介入正在跑的對話）
- 計費 / billing
