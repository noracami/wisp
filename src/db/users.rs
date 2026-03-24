use sqlx::PgPool;
use uuid::Uuid;

pub struct UserService {
    pool: PgPool,
}

impl UserService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Resolve a platform identity to a unified user ID.
    /// Creates a new user + identity if not found.
    pub async fn resolve_or_create(
        &self,
        platform: &str,
        platform_user_id: &str,
    ) -> Result<Uuid, sqlx::Error> {
        // Try to find existing identity
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT user_id FROM platform_identities
             WHERE platform = $1 AND platform_user_id = $2",
        )
        .bind(platform)
        .bind(platform_user_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((user_id,)) = existing {
            return Ok(user_id);
        }

        // Create new user + identity in a transaction
        let mut tx = self.pool.begin().await?;

        let (user_id,): (Uuid,) =
            sqlx::query_as("INSERT INTO users DEFAULT VALUES RETURNING id")
                .fetch_one(&mut *tx)
                .await?;

        sqlx::query(
            "INSERT INTO platform_identities (user_id, platform, platform_user_id)
             VALUES ($1, $2, $3)",
        )
        .bind(user_id)
        .bind(platform)
        .bind(platform_user_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(user_id)
    }
}
