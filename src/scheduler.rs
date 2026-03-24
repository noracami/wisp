use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};

use wisp::config::Config;
use wisp::platform::discord::webhook::WebhookClient;
use wisp::weather::cwa::CwaClient;

pub async fn start_scheduler(config: Arc<Config>) -> Result<JobScheduler, Box<dyn std::error::Error + Send + Sync>> {
    let sched = JobScheduler::new().await?;

    // Only schedule weather report if Discord webhook is configured
    if let Some(ref discord_config) = config.discord {
        let webhook_url = discord_config.webhook_url.clone();
        let cwa_api_key = config.cwa_api_key.clone();
        let cwa_location = config.cwa_location.clone();

        sched
            .add(Job::new_async("0 0 6 * * *", move |_uuid, _lock| {
                let url = webhook_url.clone();
                let key = cwa_api_key.clone();
                let loc = cwa_location.clone();
                Box::pin(async move {
                    if let Err(e) = send_weather_report(&key, &loc, &url).await {
                        tracing::error!("Weather report failed: {e}");
                    }
                })
            })?)
            .await?;
    }

    sched.start().await?;
    tracing::info!("Scheduler started");
    Ok(sched)
}

async fn send_weather_report(
    cwa_api_key: &str,
    cwa_location: &str,
    webhook_url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cwa = CwaClient::with_default_url(cwa_api_key);
    let forecast = cwa.fetch_forecast(cwa_location).await?;

    let webhook = WebhookClient::new(webhook_url);
    let title = format!("{} 天氣預報", forecast.location);
    let description = forecast.to_embed_description();
    webhook.send_embed(&title, &description, 0x00AAFF).await?;

    tracing::info!("Sent weather report for {cwa_location}");
    Ok(())
}
