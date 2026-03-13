# Wisp 部署前準備清單

## 1. Discord Application

- [x] 到 [Discord Developer Portal](https://discord.com/developers/applications) 建立新的 Application
- [x] General Information → 記下 **Application ID** → 填入 `.env` 的 `DISCORD_APPLICATION_ID`
- [x] General Information → 記下 **Public Key** → 填入 `.env` 的 `DISCORD_PUBLIC_KEY`
- [x] Bot 頁面 → Reset Token → 記下 **Bot Token**（只顯示一次）→ 填入 `.env` 的 `DISCORD_BOT_TOKEN`

## 2. 邀請 Bot 到伺服器

- [ ] OAuth2 → URL Generator → 勾選 scope: `bot` + `applications.commands`
- [ ] 用產生的 URL 邀請 Bot 到你的 Discord 伺服器

## 3. Discord Webhook

- [x] 到你要接收訊息的 Discord 頻道 → 頻道設定 → 整合 → Webhook
- [x] 建立 Webhook → 複製 URL → 填入 `.env` 的 `DISCORD_WEBHOOK_URL`

## 4. Anthropic API

- [ ] 到 [Anthropic Console](https://console.anthropic.com/) 取得 API Key
- [ ] 填入 `.env` 的 `ANTHROPIC_API_KEY`

## 5. CWA 氣象 API

- [ ] 到 [中央氣象署開放資料平臺](https://opendata.cwa.gov.tw/) 註冊帳號
- [ ] 登入後到「取得授權碼」頁面取得 API Key
- [ ] 填入 `.env` 的 `CWA_API_KEY`

## 6. 公開 URL（讓 Discord 連到你的服務）

- [ ] 用 ngrok 或 cloudflare tunnel 建立公開 URL：
  ```bash
  # 擇一
  ngrok http 8080
  cloudflared tunnel --url http://localhost:8080
  ```
- [ ] 回到 Developer Portal → General Information → **Interactions Endpoint URL** 填入：
  ```
  https://<your-public-url>/interactions
  ```

## 7. 啟動

```bash
# 複製環境變數範本
cp .env.example .env

# 填完上面所有值後，啟動 DB
docker compose up db -d

# 啟動服務
RUST_LOG=info cargo run
```

全部準備好後，在 Discord 輸入 `/chat message:Hello` 測試。
