pub mod memory;
pub mod token_usage;
pub mod users;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(include_str!("../../migrations/001_init.sql"))
        .execute(pool)
        .await?;
    sqlx::raw_sql(include_str!("../../migrations/002_multi_platform.sql"))
        .execute(pool)
        .await?;
    sqlx::raw_sql(include_str!("../../migrations/003_token_usage.sql"))
        .execute(pool)
        .await?;
    Ok(())
}
