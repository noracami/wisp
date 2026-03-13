為了讓這個專案的架構更具體，我將你的目標重新梳理成一份技術規格草案，並針對其中可能存在的技術細節（特別是「無 Bot 權限」與「記憶」之間的平衡）提出詢問。

🌬️ Wisp 專案定義
Wisp 是一個基於 Rust 開發的高性能 Discord AI 代理橋接器。它捨棄了傳統 Bot 的長連線架構，改採 Interactions Endpoint (Webhook-based) 實現極簡化部署，並具備多模型驅動（以 Claude 為核心）與長期對話記憶能力。

核心功能規格
1. 技術底層與效能 (The Core)
語言： 使用 Rust 打造。利用其記憶體安全與異步處理（Tokio/Axum）優勢，確保在處理高併發 Webhook 請求時具備極低的延遲。

模型驅動： 優先整合 Claude SDK (Anthropic) 構建 Agent 架構，並設計抽象層以支援 Open AI、Gemini 或其他 LLM Provider。

2. Discord 互動機制 (The Interface)
輸入 (Trigger)： 透過 Discord Slash Commands 接收使用者指令。這不需要 Message Content 權限，且完全基於 HTTP POST (Interactions Endpoint)。

輸出 (Response)： 透過 Discord Webhook URL 回傳訊息。這允許 Bot 動態改變名稱與頭像，且回應更具彈性。

3. 管理介面 (Dashboard)
Web-based Config： 提供簡易的 Web 介面，讓使用者設定 Webhook URL、API Keys、Agent 的系統提示詞（System Prompt）以及模型偏好。

4. 記憶與上下文 (Memory)
User Persistence： 即使是無狀態的 Webhook 架構，Wisp 也能識別使用者並載入對話歷史，使 AI 能記得之前的互動脈絡。

🔍 待確認的技術細節
在開始細化架構前，有幾個地方需要跟你確認，這會影響到 Rust 後端的設計：

記憶存放的位置 (Storage)：
既然 Wisp 追求輕巧，你傾向將使用者記憶（Chat History）存放在哪裡？

方案 A：本地輕量資料庫（如 SQLite），適合 Self-hosting。

方案 B：記憶體快取（如 Redis），速度最快但重啟會消失。

方案 C：雲端資料庫（如 PostgreSQL），適合 SaaS 化。

Agent 的運作方式 (Agentic Behavior)：
你提到的 Claude Agent，是否需要具備 Tool Use (Function Calling) 的能力？（例如：讓 Wisp 去爬取網頁、檢查你的 Home Lab 狀態等），還是目前僅止於「具備記憶的對話」？

部署場景：
這個專案是打算放在你的 Home Lab (Mac Mini) 自用，還是希望未來能發展成一個讓別人也能輕鬆配置的開源工具或服務？

Slash Command 的限制：
Discord 的 Interactions Endpoint 要求伺服器必須在 3 秒內 回應。由於 AI 生成內容通常超過 3 秒，架構上勢必得採用「先回傳一個延遲回應 (Defer)，再透過 Webhook 更新內容」的非同步做法，這部分你 OK 嗎？
