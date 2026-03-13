use std::sync::Arc;
use wisp::config::Config;
use axum::{Router, routing::get};
use tracing_subscriber::EnvFilter;

mod scheduler;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    dotenvy::dotenv().ok();
    let config = Arc::new(Config::from_env().expect("Missing required environment variables"));

    // Start scheduler
    let _scheduler = scheduler::start_scheduler(config.clone())
        .await
        .expect("Failed to start scheduler");

    let app = Router::new()
        .route("/health", get(|| async { "ok" }));

    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Starting Wisp on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
