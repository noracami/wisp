use std::sync::Arc;
use axum::{Router, routing::get};
use tracing_subscriber::EnvFilter;
use wisp::config::Config;
use wisp::assistant::service::Assistant;
use wisp::db::{create_pool, run_migrations};
use wisp::db::memory::Memory;
use wisp::db::token_usage::TokenUsageStore;
use wisp::db::users::UserService;
use wisp::llm::claude::ClaudeClient;
use wisp::platform::discord::handler::{DiscordState, routes as discord_routes};
use wisp::platform::line::client::LineClient;
use wisp::platform::line::handler::{LineState, routes as line_routes};
use wisp::tools::ToolRegistry;
use wisp::tools::search::SearchTool;
use wisp::tools::weather::WeatherTool;
use wisp::weather::cwa::CwaClient;

mod scheduler;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    dotenvy::dotenv().ok();
    let config = Arc::new(Config::from_env().expect("Missing required environment variables"));

    // Database
    let pool = create_pool(&config.database_url)
        .await
        .expect("Failed to connect to database");
    run_migrations(&pool)
        .await
        .expect("Failed to run migrations");

    // Shared services
    let memory = Arc::new(Memory::new(pool.clone()));
    let token_usage = Arc::new(TokenUsageStore::new(pool.clone()));
    let users = Arc::new(UserService::new(pool));
    let claude = Arc::new(ClaudeClient::with_default_url(&config.anthropic_api_key));

    // Tool registry
    let mut registry = ToolRegistry::new();
    let cwa_client = CwaClient::with_default_url(&config.cwa_api_key);
    registry.register(Box::new(WeatherTool::new(cwa_client)));
    if let Some(ref search_config) = config.google_search {
        registry.register(Box::new(SearchTool::new(
            &search_config.api_key,
            &search_config.engine_id,
        )));
        tracing::info!("Web search tool enabled");
    }
    let tools = Arc::new(registry);

    let assistant = Arc::new(Assistant::new(
        claude,
        memory.clone(),
        users.clone(),
        tools,
        token_usage,
    ));

    // Build router
    let mut app = Router::new().route("/health", get(|| async { "ok" }));

    // Discord (optional)
    if let Some(ref discord_config) = config.discord {
        let discord_state = Arc::new(DiscordState {
            public_key_hex: discord_config.public_key.clone(),
            application_id: discord_config.application_id.clone(),
            bot_token: discord_config.bot_token.clone(),
            assistant: assistant.clone(),
            users: users.clone(),
        });
        app = app.nest("/discord", discord_routes(discord_state));
        tracing::info!("Discord platform enabled");
    }

    // LINE (optional)
    if let Some(ref line_config) = config.line {
        let line_client = Arc::new(LineClient::with_default_url(&line_config.channel_access_token));
        let line_state = Arc::new(LineState {
            channel_secret: line_config.channel_secret.clone(),
            channel_access_token: line_config.channel_access_token.clone(),
            assistant: assistant.clone(),
            users: users.clone(),
            client: line_client,
        });
        app = app.nest("/line", line_routes(line_state));
        tracing::info!("LINE platform enabled");
    }

    // Scheduler
    let _scheduler = scheduler::start_scheduler(config.clone())
        .await
        .expect("Failed to start scheduler");

    // Start server
    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Starting Wisp on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
