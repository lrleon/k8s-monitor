use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, error, warn};

use crate::types::IstioMetric;

pub struct IstioMetricsClient {
    client: Client,
    prometheus_url: String,
}

impl IstioMetricsClient {
    pub fn new(prometheus_url: String) -> Self {
        Self {
            client: Client::new(),
            prometheus_url,
        }
    }

    pub async fn get_request_rate(&self, namespace: &str) -> Result<HashMap<String, f64>> {
        let query = format!(
            "sum(rate(istio_requests_total{{destination_namespace=\"{}\"}}[1m])) by (destination_pod)",
            namespace
        );

        let metrics = self.query_prometheus(&query).await?;
        self.parse_pod_metrics(metrics)
    }

    pub async fn get_latency_p50(&self, namespace: &str) -> Result<HashMap<String, f64>> {
        let query = format!(
            "histogram_quantile(0.50, sum(rate(istio_request_duration_milliseconds_bucket{{destination_namespace=\"{}\"}}[1m])) by (destination_pod, le))",
            namespace
        );

        let metrics = self.query_prometheus(&query).await?;
        self.parse_pod_metrics(metrics)
    }

    pub async fn get_latency_p95(&self, namespace: &str) -> Result<HashMap<String, f64>> {
        let query = format!(
            "histogram_quantile(0.95, sum(rate(istio_request_duration_milliseconds_bucket{{destination_namespace=\"{}\"}}[1m])) by (destination_pod, le))",
            namespace
        );

        let metrics = self.query_prometheus(&query).await?;
        self.parse_pod_metrics(metrics)
    }

    pub async fn get_latency_p99(&self, namespace: &str) -> Result<HashMap<String, f64>> {
        let query = format!(
            "histogram_quantile(0.99, sum(rate(istio_request_duration_milliseconds_bucket{{destination_namespace=\"{}\"}}[1m])) by (destination_pod, le))",
            namespace
        );

        let metrics = self.query_prometheus(&query).await?;
        self.parse_pod_metrics(metrics)
    }

    async fn query_prometheus(&self, query: &str) -> Result<Value> {
        let url = format!("{}/api/v1/query", self.prometheus_url);

        debug!("Querying Prometheus: {}", query);

        let response = self
            .client
            .get(&url)
            .query(&[("query", query)])
            .send()
            .await
            .context("Failed to send request to Prometheus")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Prometheus query failed with status {}: {}", status, body));
        }

        let json: Value = response
            .json()
            .await
            .context("Failed to parse Prometheus response as JSON")?;

        debug!("Prometheus response: {}", serde_json::to_string_pretty(&json)?);
        Ok(json)
    }

    fn parse_pod_metrics(&self, response: Value) -> Result<HashMap<String, f64>> {
        let mut metrics = HashMap::new();

        if let Some(data) = response.get("data") {
            if let Some(result) = data.get("result").and_then(|v| v.as_array()) {
                for item in result {
                    if let (Some(metric), Some(value)) = (
                        item.get("metric"),
                        item.get("value").and_then(|v| v.as_array())
                    ) {
                        // Extraer el nombre del pod de las métricas
                        let pod_name = metric
                            .get("destination_pod")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");

                        // Extraer el valor de la métrica
                        if let Some(metric_value) = value.get(1).and_then(|v| v.as_str()) {
                            match metric_value.parse::<f64>() {
                                Ok(val) => {
                                    metrics.insert(pod_name.to_string(), val);
                                }
                                Err(e) => {
                                    warn!("Failed to parse metric value '{}': {}", metric_value, e);
                                }
                            }
                        }
                    }
                }
            }
        }

        debug!("Parsed {} pod metrics", metrics.len());
        Ok(metrics)
    }

    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/api/v1/query", self.prometheus_url);

        match self
            .client
            .get(&url)
            .query(&[("query", "up")])
            .send()
            .await
        {
            Ok(response) => Ok(response.status().is_success()),
            Err(e) => {
                error!("Prometheus health check failed: {}", e);
                Ok(false)
            }
        }
    }
}

// Utility functions To parse kubernetes resources
pub fn parse_cpu_millicores(cpu_str: &str) -> Option<f64> {
    if cpu_str.ends_with('n') {
        // nanocores
        cpu_str.trim_end_matches('n').parse::<f64>().ok().map(|v| v / 1_000_000.0)
    } else if cpu_str.ends_with('u') {
        // microcores
        cpu_str.trim_end_matches('u').parse::<f64>().ok().map(|v| v / 1_000.0)
    } else if cpu_str.ends_with('m') {
        // millicores
        cpu_str.trim_end_matches('m').parse::<f64>().ok()
    } else {
        // cores
        cpu_str.parse::<f64>().ok().map(|v| v * 1000.0)
    }
}

pub fn parse_memory_bytes(memory_str: &str) -> Option<f64> {
    if memory_str.ends_with("Ki") {
        memory_str.trim_end_matches("Ki").parse::<f64>().ok().map(|v| v * 1024.0)
    } else if memory_str.ends_with("Mi") {
        memory_str.trim_end_matches("Mi").parse::<f64>().ok().map(|v| v * 1024.0 * 1024.0)
    } else if memory_str.ends_with("Gi") {
        memory_str.trim_end_matches("Gi").parse::<f64>().ok().map(|v| v * 1024.0 * 1024.0 * 1024.0)
    } else if memory_str.chars().all(|c| c.is_ascii_digit()) {
        memory_str.parse::<f64>().ok()
    } else {
        None
    }
}