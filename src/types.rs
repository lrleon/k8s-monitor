use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

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
    // Nuevas métricas agregadas de ventana temporal
    pub windowed_metrics: Option<WindowedMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowedMetrics {
    pub avg_request_rate: Option<f64>,
    pub max_request_rate: Option<f64>,
    pub min_request_rate: Option<f64>,
    pub avg_latency_p95: Option<f64>,
    pub max_latency_p95: Option<f64>,
    pub min_latency_p95: Option<f64>,
    pub samples_count: usize,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct TimeSeriesData {
    pub request_rates: VecDeque<TimestampedValue>,
    pub latency_p50: VecDeque<TimestampedValue>,
    pub latency_p95: VecDeque<TimestampedValue>,
    pub latency_p99: VecDeque<TimestampedValue>,
    pub memory_usage: VecDeque<TimestampedValue>,
    pub cpu_usage: VecDeque<TimestampedValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedValue {
    pub value: f64,
    pub timestamp: DateTime<Utc>,
}

impl TimeSeriesData {
    pub fn new() -> Self {
        Self {
            request_rates: VecDeque::new(),
            latency_p50: VecDeque::new(),
            latency_p95: VecDeque::new(),
            latency_p99: VecDeque::new(),
            memory_usage: VecDeque::new(),
            cpu_usage: VecDeque::new(),
        }
    }

    pub fn add_sample(&mut self, pod_metrics: &PodMetrics, max_samples: usize) {
        let timestamp = pod_metrics.last_updated;

        // Add samples and maintain window size
        if let Some(rate) = pod_metrics.request_rate {
            self.request_rates.push_back(TimestampedValue { value: rate, timestamp });
            if self.request_rates.len() > max_samples {
                self.request_rates.pop_front();
            }
        }

        if let Some(lat) = pod_metrics.latency_p50 {
            self.latency_p50.push_back(TimestampedValue { value: lat, timestamp });
            if self.latency_p50.len() > max_samples {
                self.latency_p50.pop_front();
            }
        }

        if let Some(lat) = pod_metrics.latency_p95 {
            self.latency_p95.push_back(TimestampedValue { value: lat, timestamp });
            if self.latency_p95.len() > max_samples {
                self.latency_p95.pop_front();
            }
        }

        if let Some(lat) = pod_metrics.latency_p99 {
            self.latency_p99.push_back(TimestampedValue { value: lat, timestamp });
            if self.latency_p99.len() > max_samples {
                self.latency_p99.pop_front();
            }
        }

        if let Some(mem) = pod_metrics.memory_usage {
            self.memory_usage.push_back(TimestampedValue { value: mem, timestamp });
            if self.memory_usage.len() > max_samples {
                self.memory_usage.pop_front();
            }
        }

        if let Some(cpu) = pod_metrics.cpu_usage {
            self.cpu_usage.push_back(TimestampedValue { value: cpu, timestamp });
            if self.cpu_usage.len() > max_samples {
                self.cpu_usage.pop_front();
            }
        }
    }

    pub fn calculate_windowed_metrics(&self) -> WindowedMetrics {
        let now = Utc::now();

        // Calculate request rate statistics
        let (avg_rr, max_rr, min_rr) = if !self.request_rates.is_empty() {
            let values: Vec<f64> = self.request_rates.iter().map(|v| v.value).collect();
            let avg = values.iter().sum::<f64>() / values.len() as f64;
            let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            (Some(avg), Some(max), Some(min))
        } else {
            (None, None, None)
        };

        // Calculate latency P95 statistics
        let (avg_lat, max_lat, min_lat) = if !self.latency_p95.is_empty() {
            let values: Vec<f64> = self.latency_p95.iter().map(|v| v.value).collect();
            let avg = values.iter().sum::<f64>() / values.len() as f64;
            let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            (Some(avg), Some(max), Some(min))
        } else {
            (None, None, None)
        };

        let window_start = self.request_rates.front()
            .or(self.latency_p95.front())
            .map(|v| v.timestamp)
            .unwrap_or(now);

        WindowedMetrics {
            avg_request_rate: avg_rr,
            max_request_rate: max_rr,
            min_request_rate: min_rr,
            avg_latency_p95: avg_lat,
            max_latency_p95: max_lat,
            min_latency_p95: min_lat,
            samples_count: self.request_rates.len().max(self.latency_p95.len()),
            window_start,
            window_end: now,
        }
    }
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
    pub kubernetes: KubernetesConfig,
    pub server: ServerConfig,
    pub prometheus: PrometheusConfig,
    pub metrics: MetricsConfig,
    pub alerting: AlertingConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesConfig {
    pub namespace: String,
    pub metrics_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusConfig {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub window_size: usize,
    pub latency_window_minutes: u64,
    pub request_rate_window_minutes: u64,
    pub percentiles: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertingConfig {
    pub enabled: bool,
    pub opsgenie: OpsGenieConfig,
    pub thresholds: AlertThresholds,
    pub conditions: AlertConditions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsGenieConfig {
    pub api_key: String,
    pub api_url: String,
    pub team: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    pub memory_threshold_bytes: f64,
    pub latency_p95_threshold_ms: f64,
    pub latency_p99_threshold_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConditions {
    pub memory_duration_seconds: u64,
    pub latency_duration_seconds: u64,
    pub alert_cooldown_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub pod_name: String,
    pub namespace: String,
    pub service_name: Option<String>,
    pub alert_type: AlertType,
    pub message: String,
    pub value: f64,
    pub threshold: f64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub status: AlertStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
    MemoryThreshold,
    LatencyP95Threshold,
    LatencyP99Threshold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertStatus {
    Active,
    Resolved,
    Suppressed,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            kubernetes: KubernetesConfig {
                namespace: "default".to_string(),
                metrics_interval: 5,
            },
            server: ServerConfig {
                port: 8080,
                host: "0.0.0.0".to_string(),
            },
            prometheus: PrometheusConfig {
                url: "http://prometheus.istio-system:9090".to_string(),
            },
            metrics: MetricsConfig {
                window_size: 60,
                latency_window_minutes: 5,
                request_rate_window_minutes: 5,
                percentiles: vec![50.0, 95.0, 99.0],
            },
            alerting: AlertingConfig {
                enabled: true,
                opsgenie: OpsGenieConfig {
                    api_key: String::new(),
                    api_url: "https://api.opsgenie.com".to_string(),
                    team: "platform-team".to_string(),
                    priority: "P3".to_string(),
                },
                thresholds: AlertThresholds {
                    memory_threshold_bytes: 536870912.0, // 512MB
                    latency_p95_threshold_ms: 500.0,
                    latency_p99_threshold_ms: 1000.0,
                },
                conditions: AlertConditions {
                    memory_duration_seconds: 30,
                    latency_duration_seconds: 60,
                    alert_cooldown_seconds: 300,
                },
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
            },
        }
    }
}