# Changelog

記錄大方向的功能變更，細節可查閱附近的 git log。

## 2026-03-25

### 新增
- **Discord 公開頻道白名單** — `allowed_channels` 表控制 guild 頻道回覆為公開或 ephemeral，DM 永遠公開
- **Global Slash Command** — `/chat` 從 Guild scope 改為 Global scope
- **User Install 支援** — 使用者可自行安裝 app，不需要 guild 管理員邀請

## 2026-03-24

### 新增
- **Token 用量追蹤** — 新增 `token_usage` 表，記錄每次互動的 input/output tokens、使用的工具等
- **回應 Footer** — 每次回覆末尾顯示模型名稱、token 用量（↑input ↓output）、使用的工具
- **CWA 地名正規化** — 自動將「台北」「台北市」等變體對應到 CWA API 要求的正體全名「臺北市」
- **Web Search 工具** — 透過 Google Custom Search API 搜尋網路資訊
- **LINE 讀取動畫** — 處理訊息時顯示 loading animation

### 變更
- LLM 模型從 claude-sonnet-4 改為 claude-haiku-4-5
- Discord system prompt 改為優先使用正體中文回應

## 2026-03-23

### 新增
- **CI/CD Pipeline** — GitHub Actions 建置 Docker image → GCP Artifact Registry → Deploy Webhook 自動部署
- **Discord 部署通知** — 建置成功/失敗時發送 Discord embed 通知
- **Deploy Webhook** — systemd webhook 服務，接收部署觸發並執行 docker compose pull + restart

### 變更
- 遷移到 GCP Artifact Registry + Workload Identity Federation（無 JSON key）
- Discord 回覆改為 server 頻道 ephemeral、DM 公開

## 2026-03-19 ~ 2026-03-22

### 新增
- **多平台架構** — Platform → Core → Tool 分層設計，平台層與核心邏輯完全分離
- **LINE Bot 整合** — Webhook 驗證（HMAC-SHA256）+ Messaging API 回覆
- **統一使用者身份** — `users` + `platform_identities` 表，跨平台身份對應
- **Tool Use** — Claude Function Calling 支援多輪工具呼叫迴圈（最多 10 輪）
- **天氣工具** — CWA API 36 小時天氣預報查詢
- **平台專屬 System Prompt** — LINE 用生活聊天小幫手人設，Discord 用通用助理

## 2026-03-13 ~ 2026-03-18

### 新增
- **專案初始化** — Rust + Axum + PostgreSQL + pgvector 基礎架構
- **Discord Slash Command** — Ed25519 簽章驗證、Deferred Response + LLM 回覆
- **Claude LLM Client** — Anthropic Messages API 整合
- **定時天氣預報** — Cron 排程，CWA API → Discord Webhook embed
- **Docker 化** — 多階段 Dockerfile + docker-compose（開發/生產）
