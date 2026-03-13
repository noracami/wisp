# Wisp

基於 Rust 開發的高效能 Discord AI 代理橋接器。捨棄傳統 Bot 的長連線架構，採用 Interactions Endpoint (Webhook-based) 實現極簡化部署，具備多模型驅動（以 Claude 為核心）與長期對話記憶能力。

## 架構決策

### Discord 互動機制

- **輸入**：透過 Discord Slash Commands 接收指令（Interactions Endpoint，純 HTTP POST）
- **輸出**：透過 Discord Webhook URL 回傳訊息（可動態改變名稱與頭像）
- **3 秒限制**：Discord 要求 3 秒內回應，採用 Defer → Webhook 更新的非同步流程

### 技術選型

| 領域 | 選擇 | 理由 |
|------|------|------|
| 語言 | Rust | 記憶體安全、高效能非同步處理 |
| HTTP 框架 | Axum (Tokio) | 高併發 Webhook 請求處理 |
| Discord API | `twilight-http` + `twilight-model` | 模組化、只引入需要的部分，不帶 gateway 依賴 |
| 簽章驗證 | `ed25519-dalek` | Interactions Endpoint 的 Ed25519 簽章驗證 |
| LLM | Claude (Anthropic) 為核心 | 設計抽象層以支援 OpenAI、Gemini 等 Provider |
| 資料庫 | PostgreSQL + pgvector | 對話歷史 + 語意記憶，一個 DB 全包 |

### 記憶系統

- **PostgreSQL + pgvector** 作為唯一存儲
- 短期上下文：載入最近 N 輪對話
- 長期語意記憶：透過 embedding + cosine similarity 搜尋相關歷史對話
- pgvector 是 PostgreSQL 擴充套件，`CREATE EXTENSION vector;` 即可啟用

### Tool Use（Function Calling）

- 僅限 allow list 內的使用者可觸發 Tool Use
- 例如：爬取網頁、檢查伺服器狀態等

### 部署

- 雲端部署，Docker Compose
- 使用 `pgvector/pgvector:pg17` 作為資料庫映像檔

## 開發階段

### Phase 1a — 定時天氣預報

- 排程定時發送天氣預報到 Discord Webhook
- 呼叫天氣 API（如 CWA 中央氣象署開放資料）

### Phase 1b — Slash Command 聊天

- 接收 Slash Command → Defer 回應 → LLM 生成 → Webhook 更新
- 具備對話記憶（短期上下文 + 長期語意記憶）

### 未來規劃

- Web Dashboard（設定 Webhook URL、API Keys、System Prompt、模型偏好）
- 多模型支援（OpenAI、Gemini）
- Tool Use 擴充（更多工具能力）
