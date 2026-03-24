# CLAUDE.md

## 專案概述

Wisp 是基於 Rust 開發的多平台 AI 助理服務，採用 Platform → Core → Tool 分層架構，支援 Discord 和 LINE。

## 開發規範

- 使用正體中文撰寫註解和文件
- Commit message 使用英文，遵循 conventional commits 格式（feat/fix/docs/refactor）

## Commit 後檢查

每次 commit 功能變更（feat/fix）後，評估是否需要：
1. 更新 `CHANGELOG.md` — 記錄大方向的新增/變更
2. 更新 `docs/TODO.md` — 標記已完成的項目，或新增待辦事項
