use anyhow::{Context, Result};
use serde::Deserialize;
use std::env;
use std::path::Path;
use tracing::{info, warn};

use crate::types::Config;

#[derive(Debug, Deserialize)]
struct ConfigFile {
    kubernetes: Option<KubernetesConfigFile>,
    server: Option<ServerConfigFile>,
    prometheus: Option<PrometheusConfigFile>,
    metrics: Option<MetricsConfigFile>,
    alerting: Option<AlertingConfigFile>,
    logging: Option<LoggingConfigFile>,
}

#[derive(Debug, Deserialize)]
struct KubernetesConfigFile {
    namespace: Option<String>,
    metrics_interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ServerConfigFile {
    port: Option<u16>,
    host: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PrometheusConfigFile {
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MetricsConfigFile {
    window_size: Option<usize>,
    latency_window_minutes: Option<u64>,
    request_rate_window_minutes: Option<u64>,
    percentiles: Option<Vec<f64>>,
}

#[derive(Debug, Deserialize)]
struct AlertingConfigFile {
    enabled: Option<bool>,
    opsgenie: Option<OpsGenieConfigFile>,
    thresholds: Option<AlertThresholdsFile>,
    conditions: Option<AlertConditionsFile>,
}

#[derive(Debug, Deserialize)]
struct OpsGenieConfigFile {
    api_key: Option<String>,
    api_url: Option<String>,
    team: Option<String>,
    priority: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AlertThresholdsFile {
    memory_threshold_bytes: Option<f64>,
    latency_p95_threshold_ms: Option<f64>,
    latency_p99_threshold_ms: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct AlertConditionsFile {
    memory_duration_seconds: Option<u64>,
    latency_duration_seconds: Option<u64>,
    alert_cooldown_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct LoggingConfigFile {
    level: Option<String>,
    format: Option<String>,
}

pub fn load_config() -> Result<Config> {
    let mut config = Config::default();

    // Try to load from config file
    let config_path = env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());

    if Path::new(&config_path).exists() {
        info!("Loading configuration from: {}", config_path);
        let file_content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path))?;

        let config_file: ConfigFile = toml::from_str(&file_content)
            .with_context(|| format!("Failed to parse config file: {}", config_path))?;

        apply_config_file(&mut config, config_file);
    } else {
        warn!("Config file not found at: {}. Using defaults and environment variables.", config_path);
    }

    // Override with environment variables
    apply_env_overrides(&mut config)?;

    // Validate configuration
    validate_config(&config)?;

    Ok(config)
}

fn apply_config_file(config: &mut Config, config_file: ConfigFile) {
    if let Some(k8s) = config_file.kubernetes {
        if let Some(namespace) = k8s.namespace {
            config.kubernetes.namespace = namespace;
        }
        if let Some(interval) = k8s.metrics_interval {
            config.kubernetes.metrics_interval = interval;
        }
    }

    if let Some(server) = config_file.server {
        if let Some(port) = server.port {
            config.server.port = port;
        }
        if let Some(host) = server.host {
            config.server.host = host;
        }
    }

    if let Some(prometheus) = config_file.prometheus {
        if let Some(url) = prometheus.url {
            config.prometheus.url = url;
        }
    }

    if let Some(metrics) = config_file.metrics {
        if let Some(window_size) = metrics.window_size {
            config.metrics.window_size = window_size;
        }
        if let Some(latency_window) = metrics.latency_window_minutes {
            config.metrics.latency_window_minutes = latency_window;
        }
        if let Some(request_rate_window) = metrics.request_rate_window_minutes {
            config.metrics.request_rate_window_minutes = request_rate_window;
        }
        if let Some(percentiles) = metrics.percentiles {
            config.metrics.percentiles = percentiles;
        }
    }

    if let Some(alerting) = config_file.alerting {
        if let Some(enabled) = alerting.enabled {
            config.alerting.enabled = enabled;
        }

        if let Some(opsgenie) = alerting.opsgenie {
            if let Some(api_key) = opsgenie.api_key {
                config.alerting.opsgenie.api_key = api_key;
            }
            if let Some(api_url) = opsgenie.api_url {
                config.alerting.opsgenie.api_url = api_url;
            }
            if let Some(team) = opsgenie.team {
                config.alerting.opsgenie.team = team;
            }
            if let Some(priority) = opsgenie.priority {
                config.alerting.opsgenie.priority = priority;
            }
        }

        if let Some(thresholds) = alerting.thresholds {
            if let Some(memory_threshold) = thresholds.memory_threshold_bytes {
                config.alerting.thresholds.memory_threshold_bytes = memory_threshold;
            }
            if let Some(latency_p95) = thresholds.latency_p95_threshold_ms {
                config.alerting.thresholds.latency_p95_threshold_ms = latency_p95;
            }
            if let Some(latency_p99) = thresholds.latency_p99_threshold_ms {
                config.alerting.thresholds.latency_p99_threshold_ms = latency_p99;
            }
        }

        if let Some(conditions) = alerting.conditions {
            if let Some(memory_duration) = conditions.memory_duration_seconds {
                config.alerting.conditions.memory_duration_seconds = memory_duration;
            }
            if let Some(latency_duration) = conditions.latency_duration_seconds {
                config.alerting.conditions.latency_duration_seconds = latency_duration;
            }
            if let Some(cooldown) = conditions.alert_cooldown_seconds {
                config.alerting.conditions.alert_cooldown_seconds = cooldown;
            }
        }
    }

    if let Some(logging) = config_file.logging {
        if let Some(level) = logging.level {
            config.logging.level = level;
        }
        if let Some(format) = logging.format {
            config.logging.format = format;
        }
    }
}

fn apply_env_overrides(config: &mut Config) -> Result<()> {
    // Kubernetes overrides
    if let Ok(namespace) = env::var("NAMESPACE") {
        config.kubernetes.namespace = namespace;
    }
    if let Ok(interval) = env::var("METRICS_INTERVAL") {
        config.kubernetes.metrics_interval = interval.parse()
            .context("Invalid METRICS_INTERVAL environment variable")?;
    }

    // Server overrides
    if let Ok(port) = env::var("SERVER_PORT") {
        config.server.port = port.parse()
            .context("Invalid SERVER_PORT environment variable")?;
    }
    if let Ok(host) = env::var("SERVER_HOST") {
        config.server.host = host;
    }

    // Prometheus overrides
    if let Ok(url) = env::var("PROMETHEUS_URL") {
        config.prometheus.url = url;
    }

    // Alerting overrides
    if let Ok(enabled) = env::var("ALERTING_ENABLED") {
        config.alerting.enabled = enabled.parse()
            .context("Invalid ALERTING_ENABLED environment variable")?;
    }

    // OpsGenie API key from environment (for security)
    if let Ok(api_key) = env::var("OPSGENIE_API_KEY") {
        config.alerting.opsgenie.api_key = api_key;
    }

    // Logging overrides
    if let Ok(level) = env::var("RUST_LOG") {
        config.logging.level = level;
    }

    Ok(())
}

fn validate_config(config: &Config) -> Result<()> {
    // Validate namespace
    if config.kubernetes.namespace.is_empty() {
        return Err(anyhow::anyhow!("Kubernetes namespace cannot be empty"));
    }

    // Validate metrics interval
    if config.kubernetes.metrics_interval == 0 {
        return Err(anyhow::anyhow!("Metrics interval must be greater than 0"));
    }

    // Validate server port
    if config.server.port == 0 {
        return Err(anyhow::anyhow!("Server port must be greater than 0"));
    }

    // Validate Prometheus URL
    if config.prometheus.url.is_empty() {
        return Err(anyhow::anyhow!("Prometheus URL cannot be empty"));
    }

    // Validate window sizes
    if config.metrics.window_size == 0 {
        return Err(anyhow::anyhow!("Metrics window size must be greater than 0"));
    }

    // Validate percentiles
    for &percentile in &config.metrics.percentiles {
        if !(0.0..=100.0).contains(&percentile) {
            return Err(anyhow::anyhow!("Percentile {} must be between 0.0 and 100.0", percentile));
        }
    }

    // Validate alerting thresholds
    if config.alerting.enabled {
        if config.alerting.thresholds.memory_threshold_bytes <= 0.0 {
            return Err(anyhow::anyhow!("Memory threshold must be greater than 0"));
        }
        if config.alerting.thresholds.latency_p95_threshold_ms <= 0.0 {
            return Err(anyhow::anyhow!("Latency P95 threshold must be greater than 0"));
        }
        if config.alerting.thresholds.latency_p99_threshold_ms <= 0.0 {
            return Err(anyhow::anyhow!("Latency P99 threshold must be greater than 0"));
        }

        if config.alerting.conditions.memory_duration_seconds == 0 {
            return Err(anyhow::anyhow!("Memory duration must be greater than 0"));
        }
        if config.alerting.conditions.latency_duration_seconds == 0 {
            return Err(anyhow::anyhow!("Latency duration must be greater than 0"));
        }
    }

    // Validate logging level
    let valid_levels = ["trace", "debug", "info", "warn", "error"];
    if !valid_levels.contains(&config.logging.level.to_lowercase().as_str()) {
        return Err(anyhow::anyhow!(
            "Invalid logging level: {}. Valid levels: {:?}",
            config.logging.level,
            valid_levels
        ));
    }

    // Validate logging format
    let valid_formats = ["json", "pretty"];
    if !valid_formats.contains(&config.logging.format.to_lowercase().as_str()) {
        return Err(anyhow::anyhow!(
            "Invalid logging format: {}. Valid formats: {:?}",
            config.logging.format,
            valid_formats
        ));
    }

    info!("Configuration validation passed");
    Ok(())
}

pub fn print_config(config: &Config) {
    info!("=== Configuration ===");
    info!("Kubernetes:");
    info!("  Namespace: {}", config.kubernetes.namespace);
    info!("  Metrics interval: {}s", config.kubernetes.metrics_interval);
    info!("Server:");
    info!("  Host: {}", config.server.host);
    info!("  Port: {}", config.server.port);
    info!("Prometheus:");
    info!("  URL: {}", config.prometheus.url);
    info!("Metrics:");
    info!("  Window size: {} samples", config.metrics.window_size);
    info!("  Latency window: {} minutes", config.metrics.latency_window_minutes);
    info!("  Request rate window: {} minutes", config.metrics.request_rate_window_minutes);
    info!("  Percentiles: {:?}", config.metrics.percentiles);
    info!("Alerting:");
    info!("  Enabled: {}", config.alerting.enabled);
    if config.alerting.enabled {
        info!("  OpsGenie team: {}", config.alerting.opsgenie.team);
        info!("  OpsGenie priority: {}", config.alerting.opsgenie.priority);
        info!("  Memory threshold: {} bytes", config.alerting.thresholds.memory_threshold_bytes);
        info!("  Latency P95 threshold: {} ms", config.alerting.thresholds.latency_p95_threshold_ms);
        info!("  Latency P99 threshold: {} ms", config.alerting.thresholds.latency_p99_threshold_ms);
        info!("  Memory duration: {}s", config.alerting.conditions.memory_duration_seconds);
        info!("  Latency duration: {}s", config.alerting.conditions.latency_duration_seconds);
        info!("  Alert cooldown: {}s", config.alerting.conditions.alert_cooldown_seconds);
        info!("  OpsGenie API key: {}", if config.alerting.opsgenie.api_key.is_empty() { "Not configured" } else { "Configured" });
    }
    info!("Logging:");
    info!("  Level: {}", config.logging.level);
    info!("  Format: {}", config.logging.format);
    info!("=====================");
}