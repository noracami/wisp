use wisp::config::Config;
use axum::{Router, routing::get};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("Missing required environment variables");

    let app = Router::new()
        .route("/health", get(|| async { "ok" }));

    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Starting Wisp on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
