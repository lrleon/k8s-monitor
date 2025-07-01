use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodMetrics {
    pub namespace: String,
    pub pod_name: String,
    pub service_name: Option<String>,
    pub cpu_usage: Option<f64>,        // en millicores
    pub memory_usage: Option<f64>,     // en bytes
    pub request_rate: Option<f64>,     // requests per second
    pub latency_p50: Option<f64>,      // en milliseconds
    pub latency_p95: Option<f64>,      // en milliseconds
    pub latency_p99: Option<f64>,      // en milliseconds
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetrics {
    pub namespace: String,
    pub service_name: String,
    pub pods: Vec<PodMetrics>,
    pub total_cpu: f64,
    pub total_memory: f64,
    pub avg_request_rate: f64,
    pub avg_latency_p50: f64,
    pub avg_latency_p95: f64,
    pub avg_latency_p99: f64,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsResponse {
    pub namespace: String,
    pub services: Vec<ServiceMetrics>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesMetric {
    pub metadata: MetricMetadata,
    pub containers: Vec<ContainerMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricMetadata {
    pub name: String,
    pub namespace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMetric {
    pub name: String,
    pub usage: ResourceUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu: Option<String>,
    pub memory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IstioMetric {
    pub metric: HashMap<String, String>,
    pub value: Vec<IstioValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IstioValue {
    pub timestamp: f64,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub namespace: String,
    pub metrics_interval: u64,
    pub server_port: u16,
    pub prometheus_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            namespace: "default".to_string(),
            metrics_interval: 5,
            server_port: 8080,
            prometheus_url: "http://prometheus.istio-system:9090".to_string(),
        }
    }
}