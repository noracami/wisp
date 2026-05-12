use chrono::{Duration, Utc};
use sqlx::PgPool;
use wisp::db::reminders::{NewReminder, Reminders};
use wisp::db::users::UserService;
use wisp::db::{create_pool, run_migrations};

async fn setup() -> (PgPool, uuid::Uuid) {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string());
    let pool = create_pool(&url).await.unwrap();
    run_migrations(&pool).await.unwrap();
    let users = UserService::new(pool.clone());
    let user_id = users
        .resolve_or_create("discord", &format!("test_{}", uuid::Uuid::new_v4()))
        .await
        .unwrap();
    (pool, user_id)
}

#[tokio::test]
#[ignore]
async fn insert_and_fetch_due() {
    let (pool, user_id) = setup().await;
    let repo = Reminders::new(pool.clone());

    let id = repo
        .insert(NewReminder {
            platform: "discord",
            guild_id: "g1",
            channel_id: "c1",
            source_message_id: Some("m1"),
            user_id,
            body: "buy milk",
            fire_at: Utc::now() - Duration::seconds(1),
        })
        .await
        .unwrap();

    let due = repo.fetch_due(10).await.unwrap();
    assert!(due.iter().any(|r| r.id == id), "inserted reminder should appear in due batch");
}

#[tokio::test]
#[ignore]
async fn future_reminder_not_due() {
    let (pool, user_id) = setup().await;
    let repo = Reminders::new(pool.clone());

    let id = repo
        .insert(NewReminder {
            platform: "discord",
            guild_id: "g1",
            channel_id: "c1",
            source_message_id: None,
            user_id,
            body: "future",
            fire_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap();

    let due = repo.fetch_due(10).await.unwrap();
    assert!(due.iter().all(|r| r.id != id));
}

#[tokio::test]
#[ignore]
async fn mark_fired_excludes_from_due() {
    let (pool, user_id) = setup().await;
    let repo = Reminders::new(pool.clone());

    let id = repo
        .insert(NewReminder {
            platform: "discord",
            guild_id: "g1",
            channel_id: "c1",
            source_message_id: None,
            user_id,
            body: "x",
            fire_at: Utc::now() - Duration::seconds(1),
        })
        .await
        .unwrap();

    repo.mark_fired(id).await.unwrap();

    let due = repo.fetch_due(10).await.unwrap();
    assert!(due.iter().all(|r| r.id != id));
}

#[tokio::test]
#[ignore]
async fn mark_failed_increments_attempts_and_excludes_after_5() {
    let (pool, user_id) = setup().await;
    let repo = Reminders::new(pool.clone());

    let id = repo
        .insert(NewReminder {
            platform: "discord",
            guild_id: "g1",
            channel_id: "c1",
            source_message_id: None,
            user_id,
            body: "x",
            fire_at: Utc::now() - Duration::seconds(1),
        })
        .await
        .unwrap();

    for _ in 0..5 {
        repo.mark_failed(id, "boom").await.unwrap();
    }

    let due = repo.fetch_due(10).await.unwrap();
    assert!(due.iter().all(|r| r.id != id), "after 5 failures it should be filtered out");
}
