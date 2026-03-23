use sqlx::PgPool;
use uuid::Uuid;
use wisp::db::{create_pool, run_migrations};
use wisp::db::memory::Memory;

async fn setup_db() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string());
    let pool = create_pool(&url).await.expect("Failed to connect to DB");
    run_migrations(&pool).await.expect("Failed to run migrations");
    pool
}

#[tokio::test]
#[ignore]
async fn conversation_lifecycle() {
    let pool = setup_db().await;
    let memory = Memory::new(pool);
    let user_id = Uuid::new_v4();

    let conv_id = memory
        .get_or_create_conversation(user_id, "channel-1", "discord")
        .await
        .unwrap();
    assert!(!conv_id.is_nil());

    // Same user/channel/platform within 30 min → same conversation
    let conv_id_again = memory
        .get_or_create_conversation(user_id, "channel-1", "discord")
        .await
        .unwrap();
    assert_eq!(conv_id, conv_id_again);
}

#[tokio::test]
#[ignore]
async fn load_recent_messages_returns_most_recent_in_order() {
    let pool = setup_db().await;
    let memory = Memory::new(pool);
    let user_id = Uuid::new_v4();

    let conv_id = memory
        .get_or_create_conversation(user_id, "channel-order", "discord")
        .await
        .unwrap();

    // Store 5 messages
    for i in 1..=5 {
        memory
            .store_message(conv_id, "user", &format!("msg-{i}"), None)
            .await
            .unwrap();
    }

    // Load last 3 — should be msg-3, msg-4, msg-5 in ASC order
    let messages = memory.load_recent_messages(conv_id, 3).await.unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].content, "msg-3");
    assert_eq!(messages[1].content, "msg-4");
    assert_eq!(messages[2].content, "msg-5");
}
