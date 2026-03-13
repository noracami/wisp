use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mock_cwa_response() -> serde_json::Value {
    json!({
        "success": "true",
        "records": {
            "datasetDescription": "一般天氣預報-今明36小時天氣預報",
            "location": [{
                "locationName": "臺北市",
                "weatherElement": [
                    {
                        "elementName": "Wx",
                        "time": [{
                            "startTime": "2026-03-13 06:00:00",
                            "endTime": "2026-03-13 18:00:00",
                            "parameter": {
                                "parameterName": "晴時多雲",
                                "parameterValue": "2"
                            }
                        }]
                    },
                    {
                        "elementName": "MinT",
                        "time": [{
                            "startTime": "2026-03-13 06:00:00",
                            "endTime": "2026-03-13 18:00:00",
                            "parameter": {
                                "parameterName": "20",
                                "parameterUnit": "C"
                            }
                        }]
                    },
                    {
                        "elementName": "MaxT",
                        "time": [{
                            "startTime": "2026-03-13 06:00:00",
                            "endTime": "2026-03-13 18:00:00",
                            "parameter": {
                                "parameterName": "28",
                                "parameterUnit": "C"
                            }
                        }]
                    },
                    {
                        "elementName": "PoP",
                        "time": [{
                            "startTime": "2026-03-13 06:00:00",
                            "endTime": "2026-03-13 18:00:00",
                            "parameter": {
                                "parameterName": "10",
                                "parameterUnit": "百分比"
                            }
                        }]
                    }
                ]
            }]
        }
    })
}

#[tokio::test]
async fn fetch_forecast_parses_cwa_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/rest/datastore/F-C0032-001"))
        .and(query_param("Authorization", "test-key"))
        .and(query_param("locationName", "臺北市"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_cwa_response()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = wisp::weather::cwa::CwaClient::new("test-key", &mock_server.uri());
    let forecast = client.fetch_forecast("臺北市").await.unwrap();

    assert_eq!(forecast.location, "臺北市");
    assert_eq!(forecast.periods[0].weather, "晴時多雲");
    assert_eq!(forecast.periods[0].min_temp, "20");
    assert_eq!(forecast.periods[0].max_temp, "28");
    assert_eq!(forecast.periods[0].rain_prob, "10");
}

#[tokio::test]
async fn format_forecast_as_embed_description() {
    let forecast = wisp::weather::cwa::Forecast {
        location: "臺北市".to_string(),
        periods: vec![wisp::weather::cwa::ForecastPeriod {
            start_time: "2026-03-13 06:00:00".to_string(),
            end_time: "2026-03-13 18:00:00".to_string(),
            weather: "晴時多雲".to_string(),
            min_temp: "20".to_string(),
            max_temp: "28".to_string(),
            rain_prob: "10".to_string(),
        }],
    };

    let text = forecast.to_embed_description();
    assert!(text.contains("晴時多雲"));
    assert!(text.contains("20"));
    assert!(text.contains("28"));
}
