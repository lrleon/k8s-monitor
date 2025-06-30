mod alerting;
mod config;
mod istio;
mod kubernetes;
mod metrics;
mod server;
mod types;

use anyhow::Result;
use std::sync::Arc;
use tracing::{error, info, Level};
use tracing_subscriber::{fmt, EnvFilter};

use crate::config::{load_config, print_config};
use crate::metrics::MetricsService;
use crate::server::start_server;

#[tokio::main]
async fn main() -> Result<()> {
    // Cargar configuración primero
    let config = match load_config() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Inicializar logging basado en la configuración
    initialize_logging(&config.logging.level, &config.logging.format)?;

    info!("Starting Kubernetes Metrics Monitor with Enhanced Features");
    info!("Features enabled:");
    info!("  ✓ Kubernetes metrics collection");
    info!("  ✓ Istio service mesh metrics");
    info!("  ✓ Windowed time-series analytics");
    info!("  ✓ OpsGenie alerting integration");

    // Mostrar configuración cargada
    print_config(&config);

    // Crear el servicio de métricas
    let metrics_service = match MetricsService::new(config.clone()).await {
        Ok(service) => Arc::new(service),
        Err(e) => {
            error!("Failed to initialize metrics service: {}", e);
            error!("Please verify:");
            error!("  1. Kubernetes cluster connectivity (kubectl access)");
            error!("  2. Metrics server is installed and running");
            error!("  3. Prometheus/Istio is accessible at: {}", config.prometheus.url);
            error!("  4. Proper RBAC permissions for namespace: {}", config.kubernetes.namespace);
            if config.alerting.enabled {
                if config.alerting.opsgenie.api_key.is_empty() {
                    error!("  5. OpsGenie API key is configured (set OPSGENIE_API_KEY environment variable)");
                } else {
                    error!("  5. OpsGenie API connectivity");
                }
            }
            std::process::exit(1);
        }
    };

    // Verificar conectividad inicial
    info!("Performing comprehensive health check...");
    if !metrics_service.health_check().await {
        error!("Initial health check failed!");
        error!("Some components may not be available:");
        error!("  - Check Kubernetes connectivity");
        error!("  - Check Prometheus/Istio connectivity");
        if config.alerting.enabled {
            error!("  - Check OpsGenie API configuration");
        }
        error!("The service will continue but some features may not work properly.");
    } else {
        info!("✓ All systems healthy!");
    }

    // Clonar el Arc para el background task
    let metrics_service_bg = Arc::clone(&metrics_service);

    // Iniciar la recolección de métricas en background
    info!("Starting background metrics collection...");
    tokio::spawn(async move {
        metrics_service_bg.start_metrics_collection().await;
    });

    // Esperar un momento para que se recojan las primeras métricas
    info!("Waiting for initial metrics collection...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Mostrar resumen inicial
    let initial_metrics = metrics_service.get_current_metrics().await;
    info!("Initial metrics summary:");
    info!("  Services discovered: {}", initial_metrics.services.len());
    info!("  Total pods: {}", initial_metrics.services.iter().map(|s| s.pods.len()).sum::<usize>());

    if config.alerting.enabled {
        let alerts = metrics_service.get_active_alerts().await;
        info!("  Active alerts: {}", alerts.len());
    }

    // Iniciar el servidor web
    info!("Starting enhanced web server...");
    if let Err(e) = start_server(
        metrics_service,
        &config.server.host,
        config.server.port
    ).await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

fn initialize_logging(level: &str, format: &str) -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false);

    match format.to_lowercase().as_str() {
        "json" => {
            subscriber
                .json()
                .init();
        }
        "pretty" | _ => {
            subscriber
                .pretty()
                .init();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Config, WindowedMetrics, PodMetrics, TimeSeriesData, TimestampedValue};
    use chrono::Utc;
    use std::collections::VecDeque;

    #[test]
    fn test_windowed_metrics_calculation() {
        let mut ts_data = TimeSeriesData::new();

        // Add some sample data
        let now = Utc::now();
        ts_data.request_rates.push_back(TimestampedValue {
            value: 10.0,
            timestamp: now - chrono::Duration::seconds(30)
        });
        ts_data.request_rates.push_back(TimestampedValue {
            value: 15.0,
            timestamp: now - chrono::Duration::seconds(20)
        });
        ts_data.request_rates.push_back(TimestampedValue {
            value: 20.0,
            timestamp: now - chrono::Duration::seconds(10)
        });

        ts_data.latency_p95.push_back(TimestampedValue {
            value: 100.0,
            timestamp: now - chrono::Duration::seconds(30)
        });
        ts_data.latency_p95.push_back(TimestampedValue {
            value: 150.0,
            timestamp: now - chrono::Duration::seconds(20)
        });
        ts_data.latency_p95.push_back(TimestampedValue {
            value: 200.0,
            timestamp: now - chrono::Duration::seconds(10)
        });

        let windowed = ts_data.calculate_windowed_metrics();

        assert!(windowed.avg_request_rate.is_some());
        assert_eq!(windowed.avg_request_rate.unwrap(), 15.0); // (10+15+20)/3
        assert_eq!(windowed.max_request_rate.unwrap(), 20.0);
        assert_eq!(windowed.min_request_rate.unwrap(), 10.0);

        assert!(windowed.avg_latency_p95.is_some());
        assert_eq!(windowed.avg_latency_p95.unwrap(), 150.0); // (100+150+200)/3
        assert_eq!(windowed.max_latency_p95.unwrap(), 200.0);
        assert_eq!(windowed.min_latency_p95.unwrap(), 100.0);

        assert_eq!(windowed.samples_count, 3);
    }

    #[test]
    fn test_time_series_window_management() {
        let mut ts_data = TimeSeriesData::new();
        let max_samples = 2;

        // Create sample pod metrics
        let pod_metric1 = create_test_pod_metrics(10.0, Some(100.0));
        let pod_metric2 = create_test_pod_metrics(20.0, Some(200.0));
        let pod_metric3 = create_test_pod_metrics(30.0, Some(300.0));

        // Add samples
        ts_data.add_sample(&pod_metric1, max_samples);
        ts_data.add_sample(&pod_metric2, max_samples);
        ts_data.add_sample(&pod_metric3, max_samples);

        // Should only keep the last 2 samples
        assert_eq!(ts_data.request_rates.len(), 2);
        assert_eq!(ts_data.latency_p95.len(), 2);

        // Should be the most recent samples
        assert_eq!(ts_data.request_rates[0].value, 20.0);
        assert_eq!(ts_data.request_rates[1].value, 30.0);
        assert_eq!(ts_data.latency_p95[0].value, 200.0);
        assert_eq!(ts_data.latency_p95[1].value, 300.0);
    }

    fn create_test_pod_metrics(request_rate: f64, latency_p95: Option<f64>) -> PodMetrics {
        PodMetrics {
            namespace: "test".to_string(),
            pod_name: "test-pod".to_string(),
            service_name: Some("test-service".to_string()),
            cpu_usage: Some(100.0),
            memory_usage: Some(1024.0 * 1024.0),
            request_rate: Some(request_rate),
            latency_p50: Some(50.0),
            latency_p95,
            latency_p99: Some(200.0),
            last_updated: Utc::now(),
            windowed_metrics: None,
        }
    }

    #[tokio::test]
    async fn test_config_loading() {
        // Test that default config loads without errors
        let config = Config::default();

        assert_eq!(config.kubernetes.namespace, "default");
        assert_eq!(config.kubernetes.metrics_interval, 5);
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.metrics.window_size, 60);
        assert!(config.alerting.enabled);
    }
}interval, 5);
assert_eq!(config.server_port, 8080);
assert_eq!(config.prometheus_url, "http://prometheus.istio-system:9090");
}

#[test]
fn test_config_from_env() {
    env::set_var("NAMESPACE", "test-namespace");
    env::set_var("METRICS_INTERVAL", "10");
    env::set_var("SERVER_PORT", "9090");
    env::set_var("PROMETHEUS_URL", "http://test-prometheus:9090");

    let config = load_config();

    assert_eq!(config.namespace, "test-namespace");
    assert_eq!(config.metrics_interval, 10);
    assert_eq!(config.server_port, 9090);
    assert_eq!(config.prometheus_url, "http://test-prometheus:9090");

    // Limpiar
    env::remove_var("NAMESPACE");
    env::remove_var("METRICS_INTERVAL");
    env::remove_var("SERVER_PORT");
    env::remove_var("PROMETHEUS_URL");
}
}