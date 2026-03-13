use reqwest::Client;
use serde::Deserialize;
use std::fmt::Write;

use crate::error::AppError;

pub struct CwaClient {
    api_key: String,
    base_url: String,
    http: Client,
}

#[derive(Debug, Clone)]
pub struct Forecast {
    pub location: String,
    pub periods: Vec<ForecastPeriod>,
}

#[derive(Debug, Clone)]
pub struct ForecastPeriod {
    pub start_time: String,
    pub end_time: String,
    pub weather: String,
    pub min_temp: String,
    pub max_temp: String,
    pub rain_prob: String,
}

impl Forecast {
    pub fn to_embed_description(&self) -> String {
        let mut desc = String::new();
        for p in &self.periods {
            writeln!(
                &mut desc,
                "**{} ~ {}**\n{} | {}°C ~ {}°C | 降雨機率 {}%\n",
                p.start_time, p.end_time, p.weather, p.min_temp, p.max_temp, p.rain_prob
            )
            .unwrap();
        }
        desc
    }
}

// CWA API response structures
#[derive(Deserialize)]
struct CwaResponse {
    records: CwaRecords,
}

#[derive(Deserialize)]
struct CwaRecords {
    location: Vec<CwaLocation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CwaLocation {
    location_name: String,
    weather_element: Vec<CwaWeatherElement>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CwaWeatherElement {
    element_name: String,
    time: Vec<CwaTimeEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CwaTimeEntry {
    start_time: String,
    end_time: String,
    parameter: CwaParameter,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CwaParameter {
    parameter_name: String,
}

impl CwaClient {
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: base_url.to_string(),
            http: Client::new(),
        }
    }

    pub fn with_default_url(api_key: &str) -> Self {
        Self::new(api_key, "https://opendata.cwa.gov.tw")
    }

    pub async fn fetch_forecast(&self, location: &str) -> Result<Forecast, AppError> {
        let resp: CwaResponse = self
            .http
            .get(format!(
                "{}/api/v1/rest/datastore/F-C0032-001",
                self.base_url
            ))
            .query(&[
                ("Authorization", &self.api_key),
                ("locationName", &location.to_string()),
            ])
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::Request)?
            .json()
            .await?;

        let loc = resp
            .records
            .location
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Internal(format!("Location {location} not found")))?;

        let find_element = |name: &str| -> Vec<(String, String, String)> {
            loc.weather_element
                .iter()
                .find(|e| e.element_name == name)
                .map(|e| {
                    e.time
                        .iter()
                        .map(|t| {
                            (
                                t.start_time.clone(),
                                t.end_time.clone(),
                                t.parameter.parameter_name.clone(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

        let wx = find_element("Wx");
        let min_t = find_element("MinT");
        let max_t = find_element("MaxT");
        let pop = find_element("PoP");

        let periods = wx
            .into_iter()
            .enumerate()
            .map(|(i, (start, end, weather))| ForecastPeriod {
                start_time: start,
                end_time: end,
                weather,
                min_temp: min_t.get(i).map(|t| t.2.clone()).unwrap_or_default(),
                max_temp: max_t.get(i).map(|t| t.2.clone()).unwrap_or_default(),
                rain_prob: pop.get(i).map(|t| t.2.clone()).unwrap_or_default(),
            })
            .collect();

        Ok(Forecast {
            location: loc.location_name,
            periods,
        })
    }
}
