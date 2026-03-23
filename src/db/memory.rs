use pgvector::Vector;
use sqlx::PgPool;
use uuid::Uuid;

use crate::platform::ChatMessage;

pub struct Memory {
    pool: PgPool,
}

impl Memory {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_or_create_conversation(
        &self,
        user_id: Uuid,
        channel_id: &str,
        platform: &str,
    ) -> Result<Uuid, sqlx::Error> {
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM conversations
             WHERE user_id = $1 AND channel_id = $2 AND platform = $3
             AND updated_at > now() - interval '30 minutes'
             ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(channel_id)
        .bind(platform)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((id,)) = existing {
            // Refresh updated_at
            sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await?;
            return Ok(id);
        }

        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO conversations (user_id, channel_id, platform)
             VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(user_id)
        .bind(channel_id)
        .bind(platform)
        .fetch_one(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn store_message(
        &self,
        conversation_id: Uuid,
        role: &str,
        content: &str,
        embedding: Option<Vec<f32>>,
    ) -> Result<(), sqlx::Error> {
        let emb = embedding.map(Vector::from);
        sqlx::query(
            "INSERT INTO messages (conversation_id, role, content, embedding)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(conversation_id)
        .bind(role)
        .bind(content)
        .bind(emb)
        .execute(&self.pool)
        .await?;

        // Update conversation's updated_at
        sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
            .bind(conversation_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn load_recent_messages(
        &self,
        conversation_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ChatMessage>, sqlx::Error> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT role, content FROM (
                SELECT role, content, created_at FROM messages
                WHERE conversation_id = $1
                ORDER BY created_at DESC
                LIMIT $2
             ) sub ORDER BY created_at ASC",
        )
        .bind(conversation_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(role, content)| ChatMessage { role, content })
            .collect())
    }

    pub async fn search_similar(
        &self,
        user_id: Uuid,
        query_embedding: Vec<f32>,
        limit: i64,
    ) -> Result<Vec<ChatMessage>, sqlx::Error> {
        let emb = Vector::from(query_embedding);
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT m.role, m.content
             FROM messages m
             JOIN conversations c ON m.conversation_id = c.id
             WHERE c.user_id = $1 AND m.embedding IS NOT NULL
             ORDER BY m.embedding <=> $2
             LIMIT $3",
        )
        .bind(user_id)
        .bind(emb)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(role, content)| ChatMessage { role, content })
            .collect())
    }
}
