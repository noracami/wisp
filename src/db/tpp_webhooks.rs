use async_trait::async_trait;
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub struct StoredWebhook {
    pub user_id: String,
    pub webhook_id: String,
    pub webhook_token: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub channel_name: Option<String>,
}

#[async_trait]
pub trait TppWebhookStore: Send + Sync {
    async fn upsert(
        &self,
        user_id: &str,
        webhook_id: &str,
        webhook_token: &str,
        channel_id: &str,
        guild_id: Option<&str>,
        channel_name: Option<&str>,
    ) -> sqlx::Result<()>;

    async fn find_by_user(&self, user_id: &str) -> sqlx::Result<Option<StoredWebhook>>;
}

pub struct TppWebhookRepo {
    pool: PgPool,
}

impl TppWebhookRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TppWebhookStore for TppWebhookRepo {
    async fn upsert(
        &self,
        user_id: &str,
        webhook_id: &str,
        webhook_token: &str,
        channel_id: &str,
        guild_id: Option<&str>,
        channel_name: Option<&str>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO tpp_webhooks
                (user_id, webhook_id, webhook_token, channel_id, guild_id, channel_name)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (user_id) DO UPDATE SET
                webhook_id    = EXCLUDED.webhook_id,
                webhook_token = EXCLUDED.webhook_token,
                channel_id    = EXCLUDED.channel_id,
                guild_id      = EXCLUDED.guild_id,
                channel_name  = EXCLUDED.channel_name,
                updated_at    = now()",
        )
        .bind(user_id)
        .bind(webhook_id)
        .bind(webhook_token)
        .bind(channel_id)
        .bind(guild_id)
        .bind(channel_name)
        .execute(&self.pool)
        .await
        .map(|_| ())
    }

    async fn find_by_user(&self, user_id: &str) -> sqlx::Result<Option<StoredWebhook>> {
        let row: Option<(String, String, String, String, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT user_id, webhook_id, webhook_token, channel_id, guild_id, channel_name
                 FROM tpp_webhooks WHERE user_id = $1",
            )
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(
            |(user_id, webhook_id, webhook_token, channel_id, guild_id, channel_name)| {
                StoredWebhook {
                    user_id,
                    webhook_id,
                    webhook_token,
                    channel_id,
                    guild_id,
                    channel_name,
                }
            },
        ))
    }
}
