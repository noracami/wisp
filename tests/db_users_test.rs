use sqlx::PgPool;
use wisp::db::{create_pool, run_migrations};
use wisp::db::users::UserService;

async fn setup_db() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string());
    let pool = create_pool(&url).await.expect("Failed to connect to DB");
    run_migrations(&pool).await.expect("Failed to run migrations");
    pool
}

#[tokio::test]
#[ignore] // Requires database
async fn resolve_or_create_creates_new_user() {
    let pool = setup_db().await;
    let svc = UserService::new(pool);

    let user_id = svc.resolve_or_create("discord", "123456").await.unwrap();
    assert!(!user_id.is_nil());

    // Same platform + platform_user_id should return same user
    let user_id_again = svc.resolve_or_create("discord", "123456").await.unwrap();
    assert_eq!(user_id, user_id_again);
}

#[tokio::test]
#[ignore] // Requires database
async fn different_platforms_create_different_users() {
    let pool = setup_db().await;
    let svc = UserService::new(pool);

    let discord_user = svc.resolve_or_create("discord", "user_abc").await.unwrap();
    let line_user = svc.resolve_or_create("line", "user_abc").await.unwrap();

    // Same platform_user_id but different platforms → different users
    assert_ne!(discord_user, line_user);
}
