use sqlx::PgPool;
use uuid::Uuid;

pub struct TokenUsageStore {
    pool: PgPool,
}

impl TokenUsageStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn record(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
        platform: &str,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        tool_iterations: u32,
        tools_used: &[String],
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO token_usage (user_id, conversation_id, platform, model, input_tokens, output_tokens, tool_iterations, tools_used)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(user_id)
        .bind(conversation_id)
        .bind(platform)
        .bind(model)
        .bind(input_tokens as i32)
        .bind(output_tokens as i32)
        .bind(tool_iterations as i32)
        .bind(tools_used)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
