use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};

use wisp::config::Config;
use wisp::discord::webhook::WebhookClient;
use wisp::weather::cwa::CwaClient;

pub async fn start_scheduler(config: Arc<Config>) -> Result<JobScheduler, Box<dyn std::error::Error + Send + Sync>> {
    let sched = JobScheduler::new().await?;

    let weather_config = config.clone();
    // Run every day at 06:00 UTC (14:00 TST)
    sched
        .add(Job::new_async("0 0 6 * * *", move |_uuid, _lock| {
            let cfg = weather_config.clone();
            Box::pin(async move {
                if let Err(e) = send_weather_report(&cfg).await {
                    tracing::error!("Weather report failed: {e}");
                }
            })
        })?)
        .await?;

    sched.start().await?;
    tracing::info!("Scheduler started");
    Ok(sched)
}

async fn send_weather_report(config: &Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cwa = CwaClient::with_default_url(&config.cwa_api_key);
    let forecast = cwa.fetch_forecast(&config.cwa_location).await?;

    let webhook = WebhookClient::new(&config.discord_webhook_url);
    let title = format!("{} 天氣預報", forecast.location);
    let description = forecast.to_embed_description();
    webhook.send_embed(&title, &description, 0x00AAFF).await?;

    tracing::info!("Sent weather report for {}", config.cwa_location);
    Ok(())
}
