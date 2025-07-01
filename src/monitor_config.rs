use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    pub general: GeneralConfig,
    pub monitoring: MonitoringConfig,
    pub default_thresholds: ThresholdConfig,
    #[serde(default)]
    pub services: HashMap<String, ServiceConfig>,
    pub notifications: NotificationConfig,
    pub severity: SeverityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(with = "duration_string")]
    pub evaluation_interval: Duration,
    pub default_namespace: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    #[serde(default)]
    pub include_services: Vec<String>,
    #[serde(default)]
    pub exclude_services: Vec<String>,
    pub window_sizes: WindowSizes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSizes {
    pub latency: usize,
    pub request_rate: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdConfig {
    pub memory: MemoryThreshold,
    pub cpu: CpuThreshold,
    pub latency: LatencyThreshold,
    pub request_rate: RequestRateThreshold,
    pub availability: AvailabilityThreshold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryThreshold {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub warning_percent: f64,
    pub critical_percent: f64,
    #[serde(with = "duration_string")]
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuThreshold {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub warning_millicores: f64,
    pub critical_millicores: f64,
    #[serde(with = "duration_string")]
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyThreshold {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub p50: LatencyLevel,
    pub p95: LatencyLevel,
    pub p99: LatencyLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyLevel {
    pub warning_ms: f64,
    pub critical_ms: f64,
    #[serde(with = "duration_string")]
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestRateThreshold {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub min_warning_rps: f64,
    pub max_warning_rps: f64,
    pub max_critical_rps: f64,
    #[serde(with = "duration_string")]
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilityThreshold {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub min_healthy_pods: u32,
    pub min_healthy_percent: f64,
    #[serde(with = "duration_string")]
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub window_sizes: Option<WindowSizes>,
    #[serde(default)]
    pub memory: Option<MemoryThreshold>,
    #[serde(default)]
    pub cpu: Option<CpuThreshold>,
    #[serde(default)]
    pub latency: Option<LatencyThreshold>,
    #[serde(default)]
    pub request_rate: Option<RequestRateThreshold>,
    #[serde(default)]
    pub availability: Option<AvailabilityThreshold>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(with = "duration_string")]
    pub global_cooldown: Duration,
    #[serde(with = "duration_string")]
    pub per_service_cooldown: Duration,
    pub channels: Vec<String>,
    pub webhook: WebhookConfig,
    pub opsgenie: OpsgenieConfig,
    pub slack: SlackConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub url: String,
    #[serde(with = "duration_string")]
    pub timeout: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsgenieConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_opsgenie_region")]
    pub region: String,
    #[serde(with = "duration_string", default = "default_timeout")]
    pub timeout: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub webhook_url: String,
    pub channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityConfig {
    pub warning: SeverityLevel,
    pub critical: SeverityLevel,
    pub info: SeverityLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityLevel {
    pub color: String,
    pub icon: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Configuration file not found: {path}")]
    FileNotFound { path: String },
    #[error("Failed to read configuration file: {source}")]
    ReadError { source: std::io::Error },
    #[error("Failed to parse TOML configuration: {source}")]
    ParseError { source: toml::de::Error },
    #[error("Invalid configuration: {message}")]
    ValidationError { message: String },
}

impl MonitorConfig {
    /// Load configuration from TOML file with comprehensive error handling
    pub fn load_from_file(path: &str) -> Result<Self, ConfigError> {
        tracing::info!("Loading monitor configuration from: {}", path);

        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => {
                    tracing::warn!("Configuration file not found at {}, using defaults", path);
                    return Ok(Self::default());
                },
                _ => return Err(ConfigError::ReadError { source: err })
            }
        };

        let config: MonitorConfig = toml::from_str(&content)
            .map_err(|source| ConfigError::ParseError { source })?;

        config.validate()?;

        tracing::info!("Configuration loaded successfully");
        tracing::debug!("Configuration: {:#?}", config);

        Ok(config)
    }

    /// Validate configuration for logical consistency
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate evaluation interval
        if self.general.evaluation_interval.as_secs() == 0 {
            return Err(ConfigError::ValidationError {
                message: "evaluation_interval must be greater than 0".to_string(),
            });
        }

        // Validate window sizes
        if self.monitoring.window_sizes.latency == 0 || self.monitoring.window_sizes.request_rate == 0 {
            return Err(ConfigError::ValidationError {
                message: "window_sizes must be greater than 0".to_string(),
            });
        }

        // Validate threshold percentages
        self.validate_threshold_config(&self.default_thresholds, "default_thresholds")?;

        // Validate service-specific configurations
        for (service_name, service_config) in &self.services {
            if let Some(memory) = &service_config.memory {
                self.validate_memory_threshold(memory, &format!("services.{}.memory", service_name))?;
            }
            if let Some(cpu) = &service_config.cpu {
                self.validate_cpu_threshold(cpu, &format!("services.{}.cpu", service_name))?;
            }
            if let Some(latency) = &service_config.latency {
                self.validate_latency_threshold(latency, &format!("services.{}.latency", service_name))?;
            }
            if let Some(request_rate) = &service_config.request_rate {
                self.validate_request_rate_threshold(request_rate, &format!("services.{}.request_rate", service_name))?;
            }
        }

        // Validate notification configuration
        if self.notifications.global_cooldown.as_secs() == 0 {
            return Err(ConfigError::ValidationError {
                message: "global_cooldown must be greater than 0".to_string(),
            });
        }

        if self.notifications.per_service_cooldown.as_secs() == 0 {
            return Err(ConfigError::ValidationError {
                message: "per_service_cooldown must be greater than 0".to_string(),
            });
        }

        Ok(())
    }

    fn validate_threshold_config(&self, config: &ThresholdConfig, prefix: &str) -> Result<(), ConfigError> {
        self.validate_memory_threshold(&config.memory, &format!("{}.memory", prefix))?;
        self.validate_cpu_threshold(&config.cpu, &format!("{}.cpu", prefix))?;
        self.validate_latency_threshold(&config.latency, &format!("{}.latency", prefix))?;
        self.validate_request_rate_threshold(&config.request_rate, &format!("{}.request_rate", prefix))?;
        Ok(())
    }

    fn validate_memory_threshold(&self, memory: &MemoryThreshold, prefix: &str) -> Result<(), ConfigError> {
        if memory.warning_percent < 0.0 || memory.warning_percent > 100.0 {
            return Err(ConfigError::ValidationError {
                message: format!("{}.warning_percent must be between 0 and 100", prefix),
            });
        }
        if memory.critical_percent < 0.0 || memory.critical_percent > 100.0 {
            return Err(ConfigError::ValidationError {
                message: format!("{}.critical_percent must be between 0 and 100", prefix),
            });
        }
        if memory.warning_percent >= memory.critical_percent {
            return Err(ConfigError::ValidationError {
                message: format!("{}.warning_percent must be less than critical_percent", prefix),
            });
        }
        Ok(())
    }

    fn validate_cpu_threshold(&self, cpu: &CpuThreshold, prefix: &str) -> Result<(), ConfigError> {
        if cpu.warning_millicores <= 0.0 {
            return Err(ConfigError::ValidationError {
                message: format!("{}.warning_millicores must be greater than 0", prefix),
            });
        }
        if cpu.critical_millicores <= 0.0 {
            return Err(ConfigError::ValidationError {
                message: format!("{}.critical_millicores must be greater than 0", prefix),
            });
        }
        if cpu.warning_millicores >= cpu.critical_millicores {
            return Err(ConfigError::ValidationError {
                message: format!("{}.warning_millicores must be less than critical_millicores", prefix),
            });
        }
        Ok(())
    }

    fn validate_latency_threshold(&self, latency: &LatencyThreshold, prefix: &str) -> Result<(), ConfigError> {
        self.validate_latency_level(&latency.p50, &format!("{}.p50", prefix))?;
        self.validate_latency_level(&latency.p95, &format!("{}.p95", prefix))?;
        self.validate_latency_level(&latency.p99, &format!("{}.p99", prefix))?;
        Ok(())
    }

    fn validate_latency_level(&self, level: &LatencyLevel, prefix: &str) -> Result<(), ConfigError> {
        if level.warning_ms <= 0.0 {
            return Err(ConfigError::ValidationError {
                message: format!("{}.warning_ms must be greater than 0", prefix),
            });
        }
        if level.critical_ms <= 0.0 {
            return Err(ConfigError::ValidationError {
                message: format!("{}.critical_ms must be greater than 0", prefix),
            });
        }
        if level.warning_ms >= level.critical_ms {
            return Err(ConfigError::ValidationError {
                message: format!("{}.warning_ms must be less than critical_ms", prefix),
            });
        }
        Ok(())
    }

    fn validate_request_rate_threshold(&self, request_rate: &RequestRateThreshold, prefix: &str) -> Result<(), ConfigError> {
        if request_rate.min_warning_rps < 0.0 {
            return Err(ConfigError::ValidationError {
                message: format!("{}.min_warning_rps must be >= 0", prefix),
            });
        }
        if request_rate.max_warning_rps <= 0.0 {
            return Err(ConfigError::ValidationError {
                message: format!("{}.max_warning_rps must be > 0", prefix),
            });
        }
        if request_rate.max_critical_rps <= 0.0 {
            return Err(ConfigError::ValidationError {
                message: format!("{}.max_critical_rps must be > 0", prefix),
            });
        }
        if request_rate.max_warning_rps >= request_rate.max_critical_rps {
            return Err(ConfigError::ValidationError {
                message: format!("{}.max_warning_rps must be less than max_critical_rps", prefix),
            });
        }
        Ok(())
    }

    /// Get effective configuration for a specific service (with overrides applied)
    pub fn get_service_config(&self, service_name: &str) -> ServiceMonitorConfig {
        let service_override = self.services.get(service_name);

        ServiceMonitorConfig {
            enabled: service_override
                .map(|s| s.enabled)
                .unwrap_or(true),

            namespace: service_override
                .and_then(|s| s.namespace.clone())
                .unwrap_or_else(|| self.general.default_namespace.clone()),

            window_sizes: service_override
                .and_then(|s| s.window_sizes.clone())
                .unwrap_or_else(|| self.monitoring.window_sizes.clone()),

            memory: service_override
                .and_then(|s| s.memory.clone())
                .unwrap_or_else(|| self.default_thresholds.memory.clone()),

            cpu: service_override
                .and_then(|s| s.cpu.clone())
                .unwrap_or_else(|| self.default_thresholds.cpu.clone()),

            latency: service_override
                .and_then(|s| s.latency.clone())
                .unwrap_or_else(|| self.default_thresholds.latency.clone()),

            request_rate: service_override
                .and_then(|s| s.request_rate.clone())
                .unwrap_or_else(|| self.default_thresholds.request_rate.clone()),

            availability: service_override
                .and_then(|s| s.availability.clone())
                .unwrap_or_else(|| self.default_thresholds.availability.clone()),
        }
    }

    /// Check if a service should be monitored based on include/exclude rules
    pub fn should_monitor_service(&self, service_name: &str) -> bool {
        // First check if service is explicitly disabled
        if let Some(service_config) = self.services.get(service_name) {
            if !service_config.enabled {
                return false;
            }
        }

        // If include_services is not empty, only monitor included services
        if !self.monitoring.include_services.is_empty() {
            return self.monitoring.include_services.contains(&service_name.to_string());
        }

        // Otherwise, monitor all services except excluded ones
        !self.monitoring.exclude_services.contains(&service_name.to_string())
    }

    /// Get list of all services that should be monitored
    pub fn get_monitored_services(&self) -> Vec<String> {
        if !self.monitoring.include_services.is_empty() {
            // Filter include_services by enabled status
            self.monitoring.include_services
                .iter()
                .filter(|service_name| self.should_monitor_service(service_name))
                .cloned()
                .collect()
        } else {
            // Return explicitly configured services that are enabled
            self.services
                .iter()
                .filter(|(service_name, config)| {
                    config.enabled && self.should_monitor_service(service_name)
                })
                .map(|(service_name, _)| service_name.clone())
                .collect()
        }
    }
}

/// Fully resolved configuration for a specific service
#[derive(Debug, Clone)]
pub struct ServiceMonitorConfig {
    pub enabled: bool,
    pub namespace: String,
    pub window_sizes: WindowSizes,
    pub memory: MemoryThreshold,
    pub cpu: CpuThreshold,
    pub latency: LatencyThreshold,
    pub request_rate: RequestRateThreshold,
    pub availability: AvailabilityThreshold,
}

impl Default for WindowSizes {
    fn default() -> Self {
        Self {
            latency: 256,
            request_rate: 256,
        }
    }
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                evaluation_interval: Duration::from_secs(60),
                default_namespace: "default".to_string(),
                enabled: true,
            },
            monitoring: MonitoringConfig {
                include_services: Vec::new(),
                exclude_services: Vec::new(),
                window_sizes: WindowSizes::default(),
            },
            default_thresholds: ThresholdConfig {
                memory: MemoryThreshold {
                    enabled: true,
                    warning_percent: 70.0,
                    critical_percent: 85.0,
                    duration: Duration::from_secs(60),
                },
                cpu: CpuThreshold {
                    enabled: true,
                    warning_millicores: 800.0,
                    critical_millicores: 950.0,
                    duration: Duration::from_secs(60),
                },
                latency: LatencyThreshold {
                    enabled: true,
                    p50: LatencyLevel {
                        warning_ms: 2000.0,
                        critical_ms: 4000.0,
                        duration: Duration::from_secs(60),
                    },
                    p95: LatencyLevel {
                        warning_ms: 3000.0,
                        critical_ms: 6000.0,
                        duration: Duration::from_secs(60),
                    },
                    p99: LatencyLevel {
                        warning_ms: 4000.0,
                        critical_ms: 8000.0,
                        duration: Duration::from_secs(60),
                    },
                },
                request_rate: RequestRateThreshold {
                    enabled: true,
                    min_warning_rps: 0.1,
                    max_warning_rps: 1000.0,
                    max_critical_rps: 2000.0,
                    duration: Duration::from_secs(60),
                },
                availability: AvailabilityThreshold {
                    enabled: true,
                    min_healthy_pods: 1,
                    min_healthy_percent: 80.0,
                    duration: Duration::from_secs(60),
                },
            },
            services: HashMap::new(),
            notifications: NotificationConfig {
                enabled: true,
                global_cooldown: Duration::from_secs(600), // 10 minutes
                per_service_cooldown: Duration::from_secs(300), // 5 minutes
                channels: vec!["log".to_string()],
                webhook: WebhookConfig {
                    enabled: false,
                    url: "".to_string(),
                    timeout: Duration::from_secs(10),
                },
                opsgenie: OpsgenieConfig {
                    enabled: false,
                    api_key: "".to_string(),
                    region: "us".to_string(),
                    timeout: Duration::from_secs(10),
                },
                slack: SlackConfig {
                    enabled: false,
                    webhook_url: "".to_string(),
                    channel: "#alerts".to_string(),
                },
            },
            severity: SeverityConfig {
                warning: SeverityLevel {
                    color: "yellow".to_string(),
                    icon: "⚠️".to_string(),
                },
                critical: SeverityLevel {
                    color: "red".to_string(),
                    icon: "🚨".to_string(),
                },
                info: SeverityLevel {
                    color: "blue".to_string(),
                    icon: "ℹ️".to_string(),
                },
            },
        }
    }
}

// Helper functions for defaults
fn default_true() -> bool {
    true
}

fn default_opsgenie_region() -> String {
    "us".to_string()
}

fn default_timeout() -> Duration {
    Duration::from_secs(10)
}

// Custom serde module for parsing duration strings like "1m", "30s"
mod duration_string {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let secs = duration.as_secs();
        if secs >= 3600 && secs % 3600 == 0 {
            serializer.serialize_str(&format!("{}h", secs / 3600))
        } else if secs >= 60 && secs % 60 == 0 {
            serializer.serialize_str(&format!("{}m", secs / 60))
        } else {
            serializer.serialize_str(&format!("{}s", secs))
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_duration(&s).map_err(serde::de::Error::custom)
    }

    fn parse_duration(s: &str) -> Result<Duration, String> {
        if s.is_empty() {
            return Err("Empty duration string".to_string());
        }

        let (number_str, unit) = if s.ends_with('s') {
            (&s[..s.len() - 1], "s")
        } else if s.ends_with('m') {
            (&s[..s.len() - 1], "m")
        } else if s.ends_with('h') {
            (&s[..s.len() - 1], "h")
        } else {
            (s, "s") // Default to seconds if no unit specified
        };

        let number: u64 = number_str.parse()
            .map_err(|_| format!("Invalid number in duration: {}", number_str))?;

        let seconds = match unit {
            "s" => number,
            "m" => number * 60,
            "h" => number * 3600,
            _ => return Err(format!("Unsupported time unit: {}", unit)),
        };

        Ok(Duration::from_secs(seconds))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_parse_duration() {
            assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
            assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
            assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
            assert_eq!(parse_duration("60").unwrap(), Duration::from_secs(60));

            assert!(parse_duration("").is_err());
            assert!(parse_duration("abc").is_err());
            assert!(parse_duration("30x").is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MonitorConfig::default();
        assert!(config.general.enabled);
        assert_eq!(config.general.evaluation_interval, Duration::from_secs(60));
        assert_eq!(config.monitoring.window_sizes.latency, 256);
    }

    #[test]
    fn test_service_config_resolution() {
        let mut config = MonitorConfig::default();

        // Add service-specific config
        let service_config = ServiceConfig {
            enabled: true,
            namespace: Some("production".to_string()),
            window_sizes: None,
            memory: Some(MemoryThreshold {
                enabled: true,
                warning_percent: 60.0,
                critical_percent: 75.0,
                duration: Duration::from_secs(60),
            }),
            cpu: None,
            latency: None,
            request_rate: None,
            availability: None,
        };

        config.services.insert("test-service".to_string(), service_config);

        let resolved = config.get_service_config("test-service");
        assert_eq!(resolved.namespace, "production");
        assert_eq!(resolved.memory.warning_percent, 60.0);
        assert_eq!(resolved.cpu.warning_millicores, 800.0); // From defaults
    }

    #[test]
    fn test_should_monitor_service() {
        let mut config = MonitorConfig::default();

        // Test include_services
        config.monitoring.include_services = vec!["service1".to_string(), "service2".to_string()];
        assert!(config.should_monitor_service("service1"));
        assert!(!config.should_monitor_service("service3"));

        // Test exclude_services
        config.monitoring.include_services.clear();
        config.monitoring.exclude_services = vec!["service1".to_string()];
        assert!(!config.should_monitor_service("service1"));
        assert!(config.should_monitor_service("service2"));
    }

    #[test]
    fn test_config_validation() {
        let mut config = MonitorConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Invalid memory percentage should fail
        config.default_thresholds.memory.warning_percent = 95.0;
        config.default_thresholds.memory.critical_percent = 85.0; // Less than warning
        assert!(config.validate().is_err());
    }
}