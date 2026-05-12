use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewReminder<'a> {
    pub platform: &'a str,
    pub guild_id: &'a str,
    pub channel_id: &'a str,
    pub source_message_id: Option<&'a str>,
    pub user_id: Uuid,
    pub body: &'a str,
    pub fire_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Reminder {
    pub id: Uuid,
    pub platform: String,
    pub guild_id: String,
    pub channel_id: String,
    pub source_message_id: Option<String>,
    pub user_id: Uuid,
    pub body: String,
    pub fire_at: DateTime<Utc>,
    pub fired_at: Option<DateTime<Utc>>,
    pub failed_attempts: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct Reminders {
    pool: PgPool,
}

impl Reminders {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 插入一筆新提醒，回傳新建記錄的 UUID。
    pub async fn insert(&self, r: NewReminder<'_>) -> Result<Uuid, sqlx::Error> {
        let row: (Uuid,) = sqlx::query_as(
            "INSERT INTO reminders
                (platform, guild_id, channel_id, source_message_id, user_id, body, fire_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id",
        )
        .bind(r.platform)
        .bind(r.guild_id)
        .bind(r.channel_id)
        .bind(r.source_message_id)
        .bind(r.user_id)
        .bind(r.body)
        .bind(r.fire_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// 取得最多 `limit` 筆已到期且尚未觸發、失敗次數 < 5 的提醒。
    /// 使用 FOR UPDATE SKIP LOCKED 避免多個 worker 重複處理同一筆資料。
    pub async fn fetch_due(&self, limit: i64) -> Result<Vec<Reminder>, sqlx::Error> {
        sqlx::query_as::<_, Reminder>(
            "SELECT * FROM reminders
             WHERE fired_at IS NULL
               AND fire_at <= now()
               AND failed_attempts < 5
             ORDER BY fire_at
             LIMIT $1
             FOR UPDATE SKIP LOCKED",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    /// 將指定提醒標記為已觸發。
    pub async fn mark_fired(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE reminders SET fired_at = now() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 記錄一次失敗，累積 failed_attempts 並存下錯誤訊息。
    pub async fn mark_failed(&self, id: Uuid, error: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE reminders
             SET failed_attempts = failed_attempts + 1,
                 last_error = $2
             WHERE id = $1",
        )
        .bind(id)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
