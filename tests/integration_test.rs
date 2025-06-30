use chrono::Utc;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use tokio::time::sleep;

const BASE_URL: &str = "http://localhost:8080";

#[tokio::test]
async fn test_health_endpoint() {
    let client = Client::new();

    let response = client
        .get(&format!("{}/health", BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["status"], "healthy");
    assert!(body["timestamp"].is_string());
}

#[tokio::test]
async fn test_root_endpoint() {
    let client = Client::new();

    let response = client
        .get(BASE_URL)
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["service"], "k8s-metrics-monitor");
    assert!(body["endpoints"].is_array());
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let client = Client::new();

    // Wait a bit for metrics to be collected
    sleep(Duration::from_secs(2)).await;

    let response = client
        .get(&format!("{}/metrics", BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["namespace"].is_string());
    assert!(body["services"].is_array());
    assert!(body["timestamp"].is_string());
}

#[tokio::test]
async fn test_services_metrics_endpoint() {
    let client = Client::new();

    let response = client
        .get(&format!("{}/metrics/services", BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["namespace"].is_string());
    assert!(body["services"].is_array());
    assert!(body["summary"].is_object());
    assert!(body["summary"]["total_services"].is_number());
}

#[tokio::test]
async fn test_raw_metrics_endpoint() {
    let client = Client::new();

    let response = client
        .get(&format!("{}/metrics/raw", BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["namespace"].is_string());
    assert!(body["pods"].is_array());
    assert!(body["timestamp"].is_string());
}

#[tokio::test]
async fn test_time_series_endpoint() {
    let client = Client::new();

    // Wait for some time series data to accumulate
    sleep(Duration::from_secs(10)).await;

    let response = client
        .get(&format!("{}/metrics/timeseries", BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["namespace"].is_string());
    assert!(body["time_series"].is_object());
    assert!(body["timestamp"].is_string());
}

#[tokio::test]
async fn test_alerts_endpoint() {
    let client = Client::new();

    let response = client
        .get(&format!("{}/alerts", BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["active_alerts"].is_array());
    assert!(body["total_alerts"].is_number());
    assert!(body["timestamp"].is_string());
}

#[tokio::test]
async fn test_metrics_with_windowed_data() {
    let client = Client::new();

    // Wait for windowed metrics to accumulate
    sleep(Duration::from_secs(15)).await;

    let response = client
        .get(&format!("{}/metrics/services", BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());

    let body: Value = response.json().await.expect("Failed to parse JSON");

    if let Some(services) = body["services"].as_array() {
        for service in services {
            if let Some(windowed_metrics) = service["windowed_metrics"].as_array() {
                if !windowed_metrics.is_empty() {
                    let windowed = &windowed_metrics[0];

                    // Check that windowed metrics have the expected structure
                    assert!(windowed["samples_count"].is_number());
                    assert!(windowed["window_start"].is_string());
                    assert!(windowed["window_end"].is_string());

                    // Optional fields should be null or numbers
                    if !windowed["avg_request_rate"].is_null() {
                        assert!(windowed["avg_request_rate"].is_number());
                        assert!(windowed["max_request_rate"].is_number());
                        assert!(windowed["min_request_rate"].is_number());
                    }

                    if !windowed["avg_latency_p95"].is_null() {
                        assert!(windowed["avg_latency_p95"].is_number());
                        assert!(windowed["max_latency_p95"].is_number());
                        assert!(windowed["min_latency_p95"].is_number());
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn test_specific_pod_time_series() {
    let client = Client::new();

    // First get the available pods
    let metrics_response = client
        .get(&format!("{}/metrics/raw", BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    let metrics_body: Value = metrics_response.json().await.expect("Failed to parse JSON");

    if let Some(pods) = metrics_body["pods"].as_array() {
        if let Some(pod) = pods.first() {
            if let Some(pod_name) = pod["pod_name"].as_str() {
                // Wait for time series data
                sleep(Duration::from_secs(5)).await;

                let ts_response = client
                    .get(&format!("{}/metrics/timeseries/{}", BASE_URL, pod_name))
                    .send()
                    .await
                    .expect("Failed to send request");

                if ts_response.status().is_success() {
                    let ts_body: Value = ts_response.json().await.expect("Failed to parse JSON");

                    assert_eq!(ts_body["pod_name"], pod_name);
                    assert!(ts_body["time_series"].is_object());
                    assert!(ts_body["timestamp"].is_string());

                    let time_series = &ts_body["time_series"];
                    assert!(time_series["request_rates"].is_array());
                    assert!(time_series["latency_p95"].is_array());
                    assert!(time_series["memory_usage"].is_array());
                    assert!(time_series["cpu_usage"].is_array());
                } else {
                    // Pod might not have time series data yet, which is ok
                    assert_eq!(ts_response.status(), 404);
                }
            }
        }
    }
}

#[tokio::test]
async fn test_endpoint_response_times() {
    let client = Client::new();
    let endpoints = vec![
        "/health",
        "/metrics",
        "/metrics/services",
        "/metrics/raw",
        "/alerts",
    ];

    for endpoint in endpoints {
        let start = std::time::Instant::now();

        let response = client
            .get(&format!("{}{}", BASE_URL, endpoint))
            .send()
            .await
            .expect("Failed to send request");

        let duration = start.elapsed();

        // All endpoints should respond within 5 seconds
        assert!(duration < Duration::from_secs(5),
                "Endpoint {} took too long: {:?}", endpoint, duration);

        assert!(response.status().is_success(),
                "Endpoint {} returned error: {}", endpoint, response.status());
    }
}

#[tokio::test]
async fn test_configuration_validation() {
    // This test validates that the configuration loading works correctly
    use k8s_metrics_monitor::config::load_config;

    // Test that default config loads without errors
    let result = std::panic::catch_unwind(|| {
        let config = k8s_metrics_monitor::types::Config::default();

        // Validate default values
        assert_eq!(config.kubernetes.namespace, "default");
        assert_eq!(config.kubernetes.metrics_interval, 5);
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.metrics.window_size, 60);
        assert!(config.alerting.enabled);
        assert_eq!(config.alerting.thresholds.memory_threshold_bytes, 536870912.0);
        assert_eq!(config.alerting.conditions.memory_duration_seconds, 30);
    });

    assert!(result.is_ok(), "Default configuration validation failed");
}

#[tokio::test]
async fn test_windowed_metrics_calculation() {
    use k8s_metrics_monitor::types::{TimeSeriesData, TimestampedValue, PodMetrics};
    use chrono::Utc;

    let mut ts_data = TimeSeriesData::new();
    let now = Utc::now();

    // Add sample data points
    for i in 0..5 {
        let timestamp = now - chrono::Duration::seconds((5 - i) * 10);
        ts_data.request_rates.push_back(TimestampedValue {
            value: (i + 1) as f64 * 10.0,
            timestamp,
        });
        ts_data.latency_p95.push_back(TimestampedValue {
            value: (i + 1) as f64 * 50.0,
            timestamp,
        });
    }

    let windowed = ts_data.calculate_windowed_metrics();

    // Verify calculations
    assert!(windowed.avg_request_rate.is_some());
    assert_eq!(windowed.avg_request_rate.unwrap(), 30.0); // (10+20+30+40+50)/5
    assert_eq!(windowed.max_request_rate.unwrap(), 50.0);
    assert_eq!(windowed.min_request_rate.unwrap(), 10.0);

    assert!(windowed.avg_latency_p95.is_some());
    assert_eq!(windowed.avg_latency_p95.unwrap(), 150.0); // (50+100+150+200+250)/5
    assert_eq!(windowed.max_latency_p95.unwrap(), 250.0);
    assert_eq!(windowed.min_latency_p95.unwrap(), 50.0);

    assert_eq!(windowed.samples_count, 5);
}

#[tokio::test]
async fn test_alert_threshold_logic() {
    // Test the alert threshold calculation logic
    use k8s_metrics_monitor::types::TimestampedValue;
    use chrono::Utc;
    use std::collections::VecDeque;

    let mut memory_data = VecDeque::new();
    let now = Utc::now();
    let threshold = 1000.0;

    // Add data points that exceed threshold for a certain duration
    for i in 0..10 {
        let timestamp = now - chrono::Duration::seconds((10 - i) * 5);
        let value = if i >= 5 { 1500.0 } else { 500.0 }; // Exceed threshold for last 5 samples

        memory_data.push_back(TimestampedValue {
            value,
            timestamp,
        });
    }

    // Calculate how long the threshold has been exceeded
    let mut consecutive_duration = 0i64;
    for sample in memory_data.iter().rev() {
        if sample.value > threshold {
            let duration = (now - sample.timestamp).num_seconds();
            consecutive_duration = consecutive_duration.max(duration);
        } else {
            break;
        }
    }

    // Should be approximately 25 seconds (5 samples * 5 seconds each)
    assert!(consecutive_duration >= 20 && consecutive_duration <= 30);
}

#[tokio::test]
async fn test_memory_usage_parsing() {
    use k8s_metrics_monitor::istio::{parse_memory_bytes, parse_cpu_millicores};

    // Test memory parsing
    assert_eq!(parse_memory_bytes("1024"), Some(1024.0));
    assert_eq!(parse_memory_bytes("1Ki"), Some(1024.0));
    assert_eq!(parse_memory_bytes("1Mi"), Some(1024.0 * 1024.0));
    assert_eq!(parse_memory_bytes("1Gi"), Some(1024.0 * 1024.0 * 1024.0));
    assert_eq!(parse_memory_bytes("invalid"), None);

    // Test CPU parsing
    assert_eq!(parse_cpu_millicores("1000m"), Some(1000.0));
    assert_eq!(parse_cpu_millicores("1"), Some(1000.0));
    assert_eq!(parse_cpu_millicores("500m"), Some(500.0));
    assert_eq!(parse_cpu_millicores("1500000n"), Some(1.5));
    assert_eq!(parse_cpu_millicores("invalid"), None);
}

// Integration test helper functions
async fn wait_for_service_ready() -> bool {
    let client = Client::new();
    let max_attempts = 30;
    let delay = Duration::from_secs(2);

    for _ in 0..max_attempts {
        if let Ok(response) = client.get(&format!("{}/health", BASE_URL)).send().await {
            if response.status().is_success() {
                return true;
            }
        }
        sleep(delay).await;
    }
    false
}

#[tokio::test]
async fn test_service_startup_time() {
    // This test ensures the service starts up within a reasonable time
    let start = std::time::Instant::now();
    let ready = wait_for_service_ready().await;
    let startup_time = start.elapsed();

    assert!(ready, "Service did not become ready within timeout");
    assert!(startup_time < Duration::from_secs(60),
            "Service took too long to start: {:?}", startup_time);
}

#[tokio::test]
async fn test_concurrent_requests() {
    let client = Client::new();
    let mut handles = Vec::new();

    // Send 10 concurrent requests to different endpoints
    for i in 0..10 {
        let client = client.clone();
        let endpoint = match i % 4 {
            0 => "/health",
            1 => "/metrics",
            2 => "/metrics/services",
            _ => "/alerts",
        };

        let handle = tokio::spawn(async move {
            client
                .get(&format!("{}{}", BASE_URL, endpoint))
                .send()
                .await
                .expect("Failed to send request")
        });

        handles.push(handle);
    }

    // Wait for all requests to complete
    for handle in handles {
        let response = handle.await.expect("Task failed");
        assert!(response.status().is_success());
    }
}

#[tokio::test]
async fn test_metrics_consistency() {
    let client = Client::new();

    // Get metrics from different endpoints and verify consistency
    let full_metrics: Value = client
        .get(&format!("{}/metrics", BASE_URL))
        .send()
        .await
        .expect("Failed to get full metrics")
        .json()
        .await
        .expect("Failed to parse JSON");

    let services_metrics: Value = client
        .get(&format!("{}/metrics/services", BASE_URL))
        .send()
        .await
        .expect("Failed to get services metrics")
        .json()
        .await
        .expect("Failed to parse JSON");

    let raw_metrics: Value = client
        .get(&format!("{}/metrics/raw", BASE_URL))
        .send()
        .await
        .expect("Failed to get raw metrics")
        .json()
        .await
        .expect("Failed to parse JSON");

    // Verify namespace consistency
    assert_eq!(full_metrics["namespace"], services_metrics["namespace"]);
    assert_eq!(full_metrics["namespace"], raw_metrics["namespace"]);

    // Verify service count consistency
    let full_service_count = full_metrics["services"].as_array().map(|s| s.len()).unwrap_or(0);
    let services_service_count = services_metrics["services"].as_array().map(|s| s.len()).unwrap_or(0);
    let summary_service_count = services_metrics["summary"]["total_services"].as_u64().unwrap_or(0) as usize;

    assert_eq!(full_service_count, services_service_count);
    assert_eq!(full_service_count, summary_service_count);
}

// Performance test
#[tokio::test]
async fn test_response_size_reasonable() {
    let client = Client::new();

    let response = client
        .get(&format!("{}/metrics", BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    let body = response.text().await.expect("Failed to get response body");

    // Response should be reasonable size (less than 10MB for normal clusters)
    assert!(body.len() < 10 * 1024 * 1024,
            "Response too large: {} bytes", body.len());

    // But should contain actual data (more than just empty structure)
    assert!(body.len() > 100, "Response too small: {} bytes", body.len());
}

#[cfg(test)]
mod test_helpers {
    use super::*;

    pub async fn create_test_pod_metrics() -> k8s_metrics_monitor::types::PodMetrics {
        use k8s_metrics_monitor::types::PodMetrics;

        PodMetrics {
            namespace: "test".to_string(),
            pod_name: "test-pod".to_string(),
            service_name: Some("test-service".to_string()),
            cpu_usage: Some(100.0),
            memory_usage: Some(1024.0 * 1024.0),
            request_rate: Some(50.0),
            latency_p50: Some(25.0),
            latency_p95: Some(75.0),
            latency_p99: Some(150.0),
            last_updated: Utc::now(),
            windowed_metrics: None,
        }
    }
}