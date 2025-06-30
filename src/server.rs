use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::metrics::MetricsService;
use crate::types::MetricsResponse;

pub struct AppState {
    pub metrics_service: Arc<MetricsService>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health_check))
        .route("/metrics", get(get_metrics))
        .route("/metrics/services", get(get_services_metrics))
        .route("/metrics/raw", get(get_raw_metrics))
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
        )
        .with_state(Arc::new(state))
}

async fn root() -> Json<Value> {
    Json(json!({
        "service": "k8s-metrics-monitor",
        "version": "0.1.0",
        "endpoints": [
            "/health",
            "/metrics",
            "/metrics/services",
            "/metrics/raw"
        ]
    }))
}

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::metrics::MetricsService;
use crate::types::{Alert, MetricsResponse};

pub struct AppState {
    pub metrics_service: Arc<MetricsService>,
}

#[derive(Debug, Deserialize)]
pub struct TimeSeriesQuery {
    pub pod: Option<String>,
    pub namespace: Option<String>,
    pub minutes: Option<u64>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health_check))
        .route("/metrics", get(get_metrics))
        .route("/metrics/services", get(get_services_metrics))
        .route("/metrics/raw", get(get_raw_metrics))
        .route("/metrics/timeseries", get(get_time_series))
        .route("/metrics/timeseries/:pod", get(get_pod_time_series))
        .route("/alerts", get(get_active_alerts))
        .route("/alerts/history", get(get_alerts_history))
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
        )
        .with_state(Arc::new(state))
}

async fn root() -> Json<Value> {
    Json(json!({
        "service": "k8s-metrics-monitor",
        "version": "0.1.0",
        "description": "Kubernetes metrics monitor with Istio integration and OpsGenie alerting",
        "endpoints": [
            "/health",
            "/metrics",
            "/metrics/services",
            "/metrics/raw",
            "/metrics/timeseries",
            "/metrics/timeseries/{pod}",
            "/alerts",
            "/alerts/history"
        ]
    }))
}

async fn health_check(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let is_healthy = state.metrics_service.health_check().await;

    if is_healthy {
        Ok(Json(json!({
            "status": "healthy",
            "timestamp": chrono::Utc::now(),
            "services": {
                "kubernetes": "connected",
                "prometheus": "connected",
                "alerting": "enabled"
            }
        })))
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn get_metrics(State(state): State<Arc<AppState>>) -> Json<MetricsResponse> {
    let metrics = state.metrics_service.get_current_metrics().await;
    Json(metrics)
}

async fn get_services_metrics(State(state): State<Arc<AppState>>) -> Json<Value> {
    let metrics = state.metrics_service.get_current_metrics().await;

    let services_summary: Vec<Value> = metrics.services
        .iter()
        .map(|service| {
            let windowed_summary: Vec<Value> = service.pods
                .iter()
                .filter_map(|pod| pod.windowed_metrics.as_ref())
                .map(|wm| json!({
                    "avg_request_rate": wm.avg_request_rate,
                    "max_request_rate": wm.max_request_rate,
                    "min_request_rate": wm.min_request_rate,
                    "avg_latency_p95": wm.avg_latency_p95,
                    "max_latency_p95": wm.max_latency_p95,
                    "min_latency_p95": wm.min_latency_p95,
                    "samples_count": wm.samples_count,
                    "window_start": wm.window_start,
                    "window_end": wm.window_end
                }))
                .collect();

            json!({
                "service_name": service.service_name,
                "namespace": service.namespace,
                "pod_count": service.pods.len(),
                "total_cpu_millicores": service.total_cpu,
                "total_memory_bytes": service.total_memory,
                "avg_request_rate": service.avg_request_rate,
                "avg_latency_p50_ms": service.avg_latency_p50,
                "avg_latency_p95_ms": service.avg_latency_p95,
                "avg_latency_p99_ms": service.avg_latency_p99,
                "last_updated": service.last_updated,
                "windowed_metrics": windowed_summary
            })
        })
        .collect();

    Json(json!({
        "namespace": metrics.namespace,
        "services": services_summary,
        "timestamp": metrics.timestamp,
        "summary": {
            "total_services": metrics.services.len(),
            "total_pods": metrics.services.iter().map(|s| s.pods.len()).sum::<usize>(),
            "total_cpu_millicores": metrics.services.iter().map(|s| s.total_cpu).sum::<f64>(),
            "total_memory_bytes": metrics.services.iter().map(|s| s.total_memory).sum::<f64>()
        }
    }))
}

async fn get_raw_metrics(State(state): State<Arc<AppState>>) -> Json<Value> {
    let metrics = state.metrics_service.get_current_metrics().await;

    let pods_detail: Vec<Value> = metrics.services
        .iter()
        .flat_map(|service| &service.pods)
        .map(|pod| json!({
            "namespace": pod.namespace,
            "pod_name": pod.pod_name,
            "service_name": pod.service_name,
            "cpu_usage_millicores": pod.cpu_usage,
            "memory_usage_bytes": pod.memory_usage,
            "request_rate_rps": pod.request_rate,
            "latency_p50_ms": pod.latency_p50,
            "latency_p95_ms": pod.latency_p95,
            "latency_p99_ms": pod.latency_p99,
            "last_updated": pod.last_updated,
            "windowed_metrics": pod.windowed_metrics
        }))
        .collect();

    Json(json!({
        "namespace": metrics.namespace,
        "pods": pods_detail,
        "timestamp": metrics.timestamp
    }))
}

async fn get_time_series(
    State(state): State<Arc<AppState>>,
    Query(params): Query<TimeSeriesQuery>,
) -> Json<Value> {
    let namespace = params.namespace.unwrap_or_else(|| "default".to_string());
    let metrics = state.metrics_service.get_current_metrics().await;

    let mut time_series_data = HashMap::new();

    for service in &metrics.services {
        if service.namespace == namespace {
            for pod in &service.pods {
                if let Some(ref filter_pod) = params.pod {
                    if pod.pod_name != *filter_pod {
                        continue;
                    }
                }

                let pod_key = format!("{}:{}", pod.namespace, pod.pod_name);
                if let Some(ts_data) = state.metrics_service.get_time_series_data(&pod_key).await {
                    time_series_data.insert(pod.pod_name.clone(), json!({
                        "request_rates": ts_data.request_rates,
                        "latency_p50": ts_data.latency_p50,
                        "latency_p95": ts_data.latency_p95,
                        "latency_p99": ts_data.latency_p99,
                        "memory_usage": ts_data.memory_usage,
                        "cpu_usage": ts_data.cpu_usage
                    }));
                }
            }
        }
    }

    Json(json!({
        "namespace": namespace,
        "time_series": time_series_data,
        "timestamp": chrono::Utc::now()
    }))
}

async fn get_pod_time_series(
    State(state): State<Arc<AppState>>,
    Path(pod_name): Path<String>,
    Query(params): Query<TimeSeriesQuery>,
) -> Result<Json<Value>, StatusCode> {
    let namespace = params.namespace.unwrap_or_else(|| "default".to_string());
    let pod_key = format!("{}:{}", namespace, pod_name);

    if let Some(ts_data) = state.metrics_service.get_time_series_data(&pod_key).await {
        Ok(Json(json!({
            "namespace": namespace,
            "pod_name": pod_name,
            "time_series": {
                "request_rates": ts_data.request_rates,
                "latency_p50": ts_data.latency_p50,
                "latency_p95": ts_data.latency_p95,
                "latency_p99": ts_data.latency_p99,
                "memory_usage": ts_data.memory_usage,
                "cpu_usage": ts_data.cpu_usage
            },
            "timestamp": chrono::Utc::now()
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn get_active_alerts(State(state): State<Arc<AppState>>) -> Json<Value> {
    let alerts = state.metrics_service.get_active_alerts().await;

    let alerts_summary: Vec<Value> = alerts
        .values()
        .map(|alert| json!({
            "id": alert.id,
            "pod_name": alert.pod_name,
            "namespace": alert.namespace,
            "service_name": alert.service_name,
            "alert_type": alert.alert_type,
            "message": alert.message,
            "value": alert.value,
            "threshold": alert.threshold,
            "first_seen": alert.first_seen,
            "last_seen": alert.last_seen,
            "status": alert.status
        }))
        .collect();

    Json(json!({
        "active_alerts": alerts_summary,
        "total_alerts": alerts.len(),
        "timestamp": chrono::Utc::now()
    }))
}

async fn get_alerts_history(State(state): State<Arc<AppState>>) -> Json<Value> {
    // Para esta implementación, solo mostramos las alertas activas
    // En una implementación completa, podrías persistir el historial en una base de datos
    let alerts = state.metrics_service.get_active_alerts().await;

    Json(json!({
        "message": "Alert history not implemented yet. Use /alerts for active alerts.",
        "active_alerts_count": alerts.len(),
        "timestamp": chrono::Utc::now()
    }))
}

pub async fn start_server(metrics_service: Arc<MetricsService>, host: &str, port: u16) -> anyhow::Result<()> {
    let app_state = AppState {
        metrics_service,
    };

    let app = create_router(app_state);
    let bind_address = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&bind_address).await?;

    info!("Server starting on {}", bind_address);
    info!("Available endpoints:");
    info!("  GET /           - Service info");
    info!("  GET /health     - Health check");
    info!("  GET /metrics    - Full metrics with windowed data");
    info!("  GET /metrics/services - Services summary with windowed metrics");
    info!("  GET /metrics/raw - Raw pod metrics with windowed data");
    info!("  GET /metrics/timeseries - Time series data for all pods");
    info!("  GET /metrics/timeseries/{{pod}} - Time series data for specific pod");
    info!("  GET /alerts     - Active alerts");
    info!("  GET /alerts/history - Alert history (placeholder)");

    axum::serve(listener, app).await?;

    Ok(())
}