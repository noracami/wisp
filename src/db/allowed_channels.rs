use sqlx::PgPool;

pub struct AllowedChannels {
    pool: PgPool,
}

impl AllowedChannels {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Check if a (guild_id, channel_id) pair is allowed for public responses.
    /// Returns true if guild is fully allowed (channel_id IS NULL) or the specific channel is allowed.
    pub async fn is_public(&self, guild_id: &str, channel_id: &str) -> bool {
        let result: Option<(bool,)> = sqlx::query_as(
            "SELECT EXISTS(
                SELECT 1 FROM allowed_channels
                WHERE guild_id = $1 AND (channel_id IS NULL OR channel_id = $2)
            )",
        )
        .bind(guild_id)
        .bind(channel_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten();

        result.map(|(exists,)| exists).unwrap_or(false)
    }
}
