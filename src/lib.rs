//! Kubernetes Metrics Monitor
//!
//! A monitoring system for Kubernetes services that tracks metrics and sends alerts
//! based on configurable thresholds.

pub mod monitor_config;
pub mod types;

// Re-export main types for easier usage
pub use monitor_config::{MonitorConfig, ServiceMonitorConfig, ConfigError, AlertSeverity};
pub use types::{PodMetrics, ServiceMetrics, MetricsResponse, Config};