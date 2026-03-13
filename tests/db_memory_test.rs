// tests/db_memory_test.rs
// This test requires: docker compose up db
// Run with: cargo test --test db_memory_test -- --ignored

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string())
}

#[tokio::test]
#[ignore]
async fn memory_store_and_retrieve_messages() {
    let pool = wisp::db::create_pool(&database_url()).await.unwrap();
    wisp::db::run_migrations(&pool).await.unwrap();

    let memory = wisp::db::memory::Memory::new(pool);

    let conv_id = memory
        .get_or_create_conversation("user123", "channel456")
        .await
        .unwrap();

    memory
        .store_message(conv_id, "user", "Hello!", None)
        .await
        .unwrap();
    memory
        .store_message(conv_id, "assistant", "Hi there!", None)
        .await
        .unwrap();

    let messages = memory.load_recent_messages(conv_id, 10).await.unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].content, "Hello!");
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(messages[1].content, "Hi there!");
}

#[tokio::test]
#[ignore]
async fn memory_reuses_recent_conversation() {
    let pool = wisp::db::create_pool(&database_url()).await.unwrap();
    wisp::db::run_migrations(&pool).await.unwrap();

    let memory = wisp::db::memory::Memory::new(pool);

    let conv1 = memory
        .get_or_create_conversation("user789", "channelABC")
        .await
        .unwrap();
    let conv2 = memory
        .get_or_create_conversation("user789", "channelABC")
        .await
        .unwrap();

    assert_eq!(conv1, conv2, "Should reuse recent conversation");
}
