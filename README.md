# Wisp

基於 Rust 開發的高效能多平台 AI 助理服務。採用分層架構，平台層（Discord、LINE）與核心邏輯完全分離，新增平台無需修改核心程式碼。

## 架構

```
Platform Layer          Core Layer           Tool Layer
┌──────────────┐    ┌──────────────────┐    ┌──────────────┐
│   Discord    │───>│                  │───>│ WeatherTool  │
│  /discord/*  │    │   Assistant      │    │              │
├──────────────┤    │                  │    ├──────────────┤
│    LINE      │───>│  Memory + LLM    │    │  (未來工具)  │
│   /line/*    │    │  Tool Call Loop  │    │              │
└──────────────┘    └──────────────────┘    └──────────────┘
```

- **Platform Layer**：處理各平台的簽章驗證、訊息解析、回覆格式
- **Core Layer**：平台無關的 Assistant，管理對話記憶與 LLM 互動
- **Tool Layer**：透過 LLM Function Calling 擴充工具能力

### 統一使用者身份

一個使用者可擁有多個平台帳號，透過 `users` + `platform_identities` 表實現跨平台身份統一。

## 技術選型

| 領域 | 選擇 | 理由 |
|------|------|------|
| 語言 | Rust 2024 | 記憶體安全、高效能非同步處理 |
| HTTP 框架 | Axum 0.8 | 高併發 Webhook 請求處理 |
| Discord | `twilight-http` + Ed25519 | Interactions Endpoint（純 HTTP，無 Gateway） |
| LINE | HMAC-SHA256 + Messaging API | Webhook 驗證 + Reply/Push 訊息 |
| LLM | Claude (Anthropic) | 支援 Tool Use（Function Calling） |
| 資料庫 | PostgreSQL + pgvector | 對話歷史 + 語意記憶 |

## 路由

| 路徑 | 說明 |
|------|------|
| `GET /health` | 健康檢查 |
| `POST /discord/interactions` | Discord Interactions Endpoint |
| `POST /line/webhook` | LINE Webhook |

## 設定

平台設定為可選，未設定的平台不載入。複製 `.env.example` 為 `.env` 並填入所需設定：

```bash
cp .env.example .env
```

必要設定：`ANTHROPIC_API_KEY`、`DATABASE_URL`、`CWA_API_KEY`

可選平台設定：
- Discord：`DISCORD_APPLICATION_ID`、`DISCORD_PUBLIC_KEY`、`DISCORD_BOT_TOKEN`、`DISCORD_WEBHOOK_URL`
- LINE：`LINE_CHANNEL_SECRET`、`LINE_CHANNEL_ACCESS_TOKEN`

## 開發

```bash
# 啟動資料庫
docker compose up db -d

# 執行測試
cargo test

# 執行需要資料庫的測試
cargo test -- --ignored

# 啟動服務
cargo run
```

## 開發階段

### 已完成

- 定時天氣預報（CWA API → Discord Webhook）
- Discord Slash Command 聊天（Defer → LLM → 回覆）
- 多平台架構重構：Platform → Core → Tool 分層設計
- LINE Bot 整合（Webhook 驗證 + Messaging API）
- Tool Use（LLM Function Calling，多輪工具呼叫迴圈）
- 統一使用者身份系統
- Web Search 工具（Google Custom Search API）
- CI/CD 自動部署（GitHub Actions → GCP Artifact Registry → Deploy Webhook）
- Token 用量追蹤（`token_usage` 表）

### 未來規劃

- Web Dashboard（設定管理）
- 多模型支援（OpenAI、Gemini）
- Tool Use 擴充（更多工具能力）
- 帳號綁定指令（跨平台帳號關聯）
- User Install + Global Slash Command
- Discord 公開頻道白名單

詳細變更紀錄見 [CHANGELOG.md](CHANGELOG.md)。
