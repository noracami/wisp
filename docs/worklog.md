# Worklog

## 2026-03-19

### 決策：Wisp 架構轉型

Wisp 從「Discord 專用 AI 聊天機器人」轉型為**平台無關的 AI 助理服務**。

**核心架構決策：**
- **分層設計（方案 A）**：Platform Layer → Core Layer → Tool Layer
- Discord 和 LINE 只是前端通道，核心邏輯完全不依賴任何平台
- 統一使用者身份系統：一個 user 可擁有多個平台帳號（`platform_identities` 表）
- 帳號綁定先用後台手動設定，未來再做指令綁定
- 功能擴充透過 **LLM Tool Use**（function calling），LLM 自行決定何時呼叫哪個工具
- 路由加 prefix：`/discord/interactions`、`/line/webhook`
- 平台設定為可選，未設定的平台不載入
- 未來遷移到 Event Bus 架構（方案 C）成本低

### 產出

1. **設計文件** — `docs/superpowers/specs/2026-03-19-multi-platform-assistant-design.md`
   - 通過 2 輪 spec review
   - 涵蓋：分層架構、統一訊息格式、DB schema、Tool Use、路由、啟動流程

2. **實作計畫** — `docs/superpowers/plans/2026-03-19-multi-platform-assistant.md`
   - 通過 2 輪 plan review
   - 11 個 Task，每個有完整程式碼、測試、commit 指令
   - 關鍵修正：`core` 模組改名 `assistant`（避免 Rust 命名衝突）、multi-turn tool call context 累積、LINE HMAC constant-time 比對

3. **Feature branch** — `feat/multi-platform-assistant`（已建立，尚未開始實作）

### 現有程式碼盤點

| 功能 | 狀態 |
|------|------|
| 天氣預報 scheduler | ✅ 已在運行 |
| Claude LLM 客戶端 | ⚠️ 程式碼存在，未掛路由 |
| 對話記憶 | ⚠️ 程式碼存在，未接入流程 |
| Discord interaction handler | ⚠️ 程式碼存在，未掛路由 |

### 下次接續

從 Task 1 開始執行實作計畫（subagent-driven development）。

---

## 2026-03-23

### 完成：多平台架構實作（Task 2–11）

一次完成實作計畫中剩餘的 10 個 Task（Task 1 在 3/19 已完成），共 12 個 commit。

**實作進度：**

| Task | 內容 | 狀態 |
|------|------|------|
| 1 | DB schema — users + platform_identities | ✅（3/19 已完成） |
| 2 | Platform types 與共用型別 | ✅ |
| 3 | Memory 改用統一 user_id | ✅ |
| 4 | Tool trait + ToolRegistry + WeatherTool | ✅ |
| 5 | Claude client — tool use 支援 | ✅ |
| 6 | Assistant service（多輪 tool call loop） | ✅ |
| 7 | 搬移 Discord 模組至 platform/discord/ | ✅ |
| 8 | Config — 可選平台設定 + LineConfig | ✅ |
| 9 | LINE Bot — HMAC 簽章驗證 + Messaging API client | ✅ |
| 10 | 接線 main.rs（可選 Discord/LINE 路由） | ✅ |
| 11 | 更新 README | ✅ |

**測試：25 個非 DB 測試全部通過，6 個 DB 整合測試待部署後驗證。**

### GCP VM 部署準備

- GCP Compute Engine VM（e2-small, Ubuntu 24.04）已建立
- 已安裝：Docker 29.3.0、Docker Compose 5.1.1、cloudflared 2026.3.0
- SSH 連線設定完成（`ssh tmp-wisp`）
- 分支已 push 到 origin

### 下次接續

1. **VM 上 clone repo** — repo 是 private，需設定 deploy key 或 PAT
2. **設定 `.env`** — 填入 Anthropic、CWA、Discord/LINE credentials
3. **`docker compose up -d`** — 啟動服務
4. **設定 Cloudflare Tunnel** — `cloudflared tunnel login` → 建立 tunnel → 指向 localhost:8080
5. **跑 DB 整合測試** — 確認 migration 和 UserService/Memory 正常
6. **設定平台 Webhook URL** — Discord Developer Portal / LINE Developers Console
7. **端對端測試** — 實際從 Discord/LINE 發訊息驗證完整流程
