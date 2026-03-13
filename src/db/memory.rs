use pgvector::Vector;
use sqlx::PgPool;
use uuid::Uuid;

pub struct Memory {
    pool: PgPool,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl Memory {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get or create a conversation for a user in a channel.
    pub async fn get_or_create_conversation(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<Uuid, sqlx::Error> {
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM conversations
             WHERE discord_user_id = $1 AND discord_channel_id = $2
             AND created_at > now() - interval '30 minutes'
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(channel_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((id,)) = existing {
            return Ok(id);
        }

        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO conversations (discord_user_id, discord_channel_id)
             VALUES ($1, $2) RETURNING id",
        )
        .bind(user_id)
        .bind(channel_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(id)
    }

    /// Store a message in a conversation.
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
        Ok(())
    }

    /// Load recent messages from a conversation (short-term context).
    pub async fn load_recent_messages(
        &self,
        conversation_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ChatMessage>, sqlx::Error> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT role, content FROM messages
             WHERE conversation_id = $1
             ORDER BY created_at ASC
             LIMIT $2",
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

    /// Semantic search across all messages for a user (Phase 2).
    pub async fn search_similar(
        &self,
        user_id: &str,
        query_embedding: Vec<f32>,
        limit: i64,
    ) -> Result<Vec<ChatMessage>, sqlx::Error> {
        let emb = Vector::from(query_embedding);
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT m.role, m.content
             FROM messages m
             JOIN conversations c ON m.conversation_id = c.id
             WHERE c.discord_user_id = $1 AND m.embedding IS NOT NULL
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
