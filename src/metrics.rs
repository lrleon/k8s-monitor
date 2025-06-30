use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use crate::alerting::AlertManager;
use crate::istio::{parse_cpu_millicores, parse_memory_bytes, IstioMetricsClient};
use crate::kubernetes::KubernetesClient;
use crate::types::{Config, MetricsResponse, PodMetrics, ServiceMetrics, TimeSeriesData};

pub struct MetricsService {
    kubernetes_client: KubernetesClient,
    istio_client: IstioMetricsClient,
    alert_manager: AlertManager,
    config: Config,
    metrics_cache: Arc<RwLock<MetricsResponse>>,
    time_series_cache: Arc<RwLock<HashMap<String, TimeSeriesData>>>,
}

impl MetricsService {
    pub async fn new(config: Config) -> Result<Self> {
        let kubernetes_client = KubernetesClient::new(config.kubernetes.namespace.clone()).await?;
        let istio_client = IstioMetricsClient::new(config.prometheus.url.clone());
        let alert_manager = AlertManager::new(config.clone());

        let initial_metrics = MetricsResponse {
            namespace: config.kubernetes.namespace.clone(),
            services: Vec::new(),
            timestamp: Utc::now(),
        };

        let metrics_cache = Arc::new(RwLock::new(initial_metrics));
        let time_series_cache = Arc::new(RwLock::new(HashMap::new()));

        Ok(Self {
            kubernetes_client,
            istio_client,
            alert_manager,
            config,
            metrics_cache,
            time_series_cache,
        })
    }

    pub async fn start_metrics_collection(&self) {
        info!("Starting metrics collection every {} seconds", self.config.kubernetes.metrics_interval);

        let mut interval = interval(Duration::from_secs(self.config.kubernetes.metrics_interval));

        loop {
            interval.tick().await;

            match self.collect_all_metrics().await {
                Ok(metrics) => {
                    // Update metrics cache
                    let mut cache = self.metrics_cache.write().await;
                    *cache = metrics.clone();

                    // Process alerts
                    let all_pods: Vec<PodMetrics> = metrics.services
                        .iter()
                        .flat_map(|s| &s.pods)
                        .cloned()
                        .collect();

                    self.alert_manager.process_metrics(&all_pods).await;

                    debug!("Updated metrics cache and processed alerts successfully");
                }
                Err(e) => {
                    error!("Failed to collect metrics: {}", e);
                }
            }
        }
    }

    pub async fn get_current_metrics(&self) -> MetricsResponse {
        let cache = self.metrics_cache.read().await;
        cache.clone()
    }

    pub async fn get_time_series_data(&self, pod_key: &str) -> Option<TimeSeriesData> {
        self.alert_manager.get_time_series_data(pod_key).await
    }

    pub async fn get_active_alerts(&self) -> HashMap<String, crate::types::Alert> {
        self.alert_manager.get_active_alerts().await
    }

    async fn collect_all_metrics(&self) -> Result<MetricsResponse> {
        info!("Collecting metrics for namespace: {}", self.config.kubernetes.namespace);

        // Obtener información de servicios y pods
        let services_with_pods = self.kubernetes_client.get_services_with_pods().await?;
        let pod_metrics = self.kubernetes_client.get_pod_metrics().await?;

        // Obtener métricas de Istio
        let (request_rates, latency_p50, latency_p95, latency_p99) = tokio::try_join!(
            self.istio_client.get_request_rate(&self.config.kubernetes.namespace),
            self.istio_client.get_latency_p50(&self.config.kubernetes.namespace),
            self.istio_client.get_latency_p95(&self.config.kubernetes.namespace),
            self.istio_client.get_latency_p99(&self.config.kubernetes.namespace),
        )?;

        // Crear un mapa de métricas de pods de Kubernetes
        let mut k8s_metrics_map: HashMap<String, &crate::types::KubernetesMetric> = HashMap::new();
        for metric in &pod_metrics {
            k8s_metrics_map.insert(metric.metadata.name.clone(), metric);
        }

        // Obtener cache de time series
        let mut ts_cache = self.time_series_cache.write().await;

        let mut service_metrics = Vec::new();

        for (service_name, pod_names) in services_with_pods {
            if pod_names.is_empty() {
                continue;
            }

            let mut pods = Vec::new();
            let mut total_cpu = 0.0;
            let mut total_memory = 0.0;
            let mut total_request_rate = 0.0;
            let mut total_latency_p50 = 0.0;
            let mut total_latency_p95 = 0.0;
            let mut total_latency_p99 = 0.0;
            let mut valid_pods = 0;

            for pod_name in &pod_names {
                let mut cpu_usage = None;
                let mut memory_usage = None;

                // Obtener métricas de recursos de Kubernetes
                if let Some(k8s_metric) = k8s_metrics_map.get(pod_name) {
                    for container in &k8s_metric.containers {
                        if let Some(cpu_str) = &container.usage.cpu {
                            if let Some(cpu_val) = parse_cpu_millicores(cpu_str) {
                                cpu_usage = Some(cpu_usage.unwrap_or(0.0) + cpu_val);
                            }
                        }
                        if let Some(memory_str) = &container.usage.memory {
                            if let Some(memory_val) = parse_memory_bytes(memory_str) {
                                memory_usage = Some(memory_usage.unwrap_or(0.0) + memory_val);
                            }
                        }
                    }
                }

                // Obtener métricas de Istio
                let request_rate = request_rates.get(pod_name).copied();
                let latency_p50_val = latency_p50.get(pod_name).copied();
                let latency_p95_val = latency_p95.get(pod_name).copied();
                let latency_p99_val = latency_p99.get(pod_name).copied();

                // Crear métricas del pod
                let mut pod_metric = PodMetrics {
                    namespace: self.config.kubernetes.namespace.clone(),
                    pod_name: pod_name.clone(),
                    service_name: Some(service_name.clone()),
                    cpu_usage,
                    memory_usage,
                    request_rate,
                    latency_p50: latency_p50_val,
                    latency_p95: latency_p95_val,
                    latency_p99: latency_p99_val,
                    last_updated: Utc::now(),
                    windowed_metrics: None,
                };

                // Actualizar time series data y calcular métricas de ventana
                let pod_key = format!("{}:{}", pod_metric.namespace, pod_metric.pod_name);
                let pod_ts = ts_cache.entry(pod_key.clone()).or_insert_with(TimeSeriesData::new);
                pod_ts.add_sample(&pod_metric, self.config.metrics.window_size);

                // Calcular métricas de ventana
                let windowed = pod_ts.calculate_windowed_metrics();
                pod_metric.windowed_metrics = Some(windowed);

                // Acumular totales para métricas del servicio
                if let Some(cpu) = cpu_usage {
                    total_cpu += cpu;
                }
                if let Some(memory) = memory_usage {
                    total_memory += memory;
                }
                if let Some(rate) = request_rate {
                    total_request_rate += rate;
                }
                if let Some(lat) = latency_p50_val {
                    total_latency_p50 += lat;
                    valid_pods += 1;
                }
                if let Some(lat) = latency_p95_val {
                    total_latency_p95 += lat;
                }
                if let Some(lat) = latency_p99_val {
                    total_latency_p99 += lat;
                }

                pods.push(pod_metric);
            }

            // Calcular promedios
            let pod_count = pod_names.len() as f64;
            let valid_pod_count = if valid_pods > 0 { valid_pods as f64 } else { 1.0 };

            let service_metric = ServiceMetrics {
                namespace: self.config.kubernetes.namespace.clone(),
                service_name: service_name.clone(),
                pods,
                total_cpu,
                total_memory,
                avg_request_rate: total_request_rate / pod_count,
                avg_latency_p50: total_latency_p50 / valid_pod_count,
                avg_latency_p95: total_latency_p95 / valid_pod_count,
                avg_latency_p99: total_latency_p99 / valid_pod_count,
                last_updated: Utc::now(),
            };

            service_metrics.push(service_metric);
        }

        // Cleanup old time series data
        self.cleanup_old_time_series(&mut ts_cache).await;

        info!("Collected metrics for {} services", service_metrics.len());

        Ok(MetricsResponse {
            namespace: self.config.kubernetes.namespace.clone(),
            services: service_metrics,
            timestamp: Utc::now(),
        })
    }

    async fn cleanup_old_time_series(&self, ts_cache: &mut HashMap<String, TimeSeriesData>) {
        let retention_duration = chrono::Duration::minutes(
            self.config.metrics.latency_window_minutes.max(
                self.config.metrics.request_rate_window_minutes
            ) as i64
        );
        let cutoff_time = Utc::now() - retention_duration;

        // Remove old samples from all time series
        for (_, data) in ts_cache.iter_mut() {
            data.request_rates.retain(|sample| sample.timestamp > cutoff_time);
            data.latency_p50.retain(|sample| sample.timestamp > cutoff_time);
            data.latency_p95.retain(|sample| sample.timestamp > cutoff_time);
            data.latency_p99.retain(|sample| sample.timestamp > cutoff_time);
            data.memory_usage.retain(|sample| sample.timestamp > cutoff_time);
            data.cpu_usage.retain(|sample| sample.timestamp > cutoff_time);
        }

        // Remove pods that have no recent data
        ts_cache.retain(|_, data| {
            !data.request_rates.is_empty()
                || !data.latency_p95.is_empty()
                || !data.memory_usage.is_empty()
        });
    }

    pub async fn health_check(&self) -> bool {
        // Verificar conectividad con Kubernetes
        let k8s_healthy = match self.kubernetes_client.get_services().await {
            Ok(_) => true,
            Err(e) => {
                warn!("Kubernetes health check failed: {}", e);
                false
            }
        };

        // Verificar conectividad con Prometheus/Istio
        let prometheus_healthy = match self.istio_client.health_check().await {
            Ok(healthy) => healthy,
            Err(e) => {
                warn!("Prometheus health check failed: {}", e);
                false
            }
        };

        // Verificar sistema de alertas
        let alerting_healthy = self.alert_manager.health_check().await;

        k8s_healthy && prometheus_healthy && alerting_healthy
    }
}_millicores, parse_memory_bytes, IstioMetricsClient};
use crate::kubernetes::KubernetesClient;
use crate::types::{Config, MetricsResponse, PodMetrics, ServiceMetrics};

pub struct MetricsService {
    kubernetes_client: KubernetesClient,
    istio_client: IstioMetricsClient,
    config: Config,
    metrics_cache: Arc<RwLock<MetricsResponse>>,
}

impl MetricsService {
    pub async fn new(config: Config) -> Result<Self> {
        let kubernetes_client = KubernetesClient::new(config.namespace.clone()).await?;
        let istio_client = IstioMetricsClient::new(config.prometheus_url.clone());

        let initial_metrics = MetricsResponse {
            namespace: config.namespace.clone(),
            services: Vec::new(),
            timestamp: Utc::now(),
        };

        let metrics_cache = Arc::new(RwLock::new(initial_metrics));

        Ok(Self {
            kubernetes_client,
            istio_client,
            config,
            metrics_cache,
        })
    }

    pub async fn start_metrics_collection(&self) {
        info!("Starting metrics collection every {} seconds", self.config.metrics_interval);

        let mut interval = interval(Duration::from_secs(self.config.metrics_interval));

        loop {
            interval.tick().await;

            match self.collect_all_metrics().await {
                Ok(metrics) => {
                    let mut cache = self.metrics_cache.write().await;
                    *cache = metrics;
                    debug!("Updated metrics cache successfully");
                }
                Err(e) => {
                    error!("Failed to collect metrics: {}", e);
                }
            }
        }
    }

    pub async fn get_current_metrics(&self) -> MetricsResponse {
        let cache = self.metrics_cache.read().await;
        cache.clone()
    }

    async fn collect_all_metrics(&self) -> Result<MetricsResponse> {
        info!("Collecting metrics for namespace: {}", self.config.namespace);

        // Obtener información de servicios y pods
        let services_with_pods = self.kubernetes_client.get_services_with_pods().await?;
        let pod_metrics = self.kubernetes_client.get_pod_metrics().await?;

        // Obtener métricas de Istio
        let (request_rates, latency_p50, latency_p95, latency_p99) = tokio::try_join!(
            self.istio_client.get_request_rate(&self.config.namespace),
            self.istio_client.get_latency_p50(&self.config.namespace),
            self.istio_client.get_latency_p95(&self.config.namespace),
            self.istio_client.get_latency_p99(&self.config.namespace),
        )?;

        // Crear un mapa de métricas de pods de Kubernetes
        let mut k8s_metrics_map: HashMap<String, &crate::types::KubernetesMetric> = HashMap::new();
        for metric in &pod_metrics {
            k8s_metrics_map.insert(metric.metadata.name.clone(), metric);
        }

        let mut service_metrics = Vec::new();

        for (service_name, pod_names) in services_with_pods {
            if pod_names.is_empty() {
                continue;
            }

            let mut pods = Vec::new();
            let mut total_cpu = 0.0;
            let mut total_memory = 0.0;
            let mut total_request_rate = 0.0;
            let mut total_latency_p50 = 0.0;
            let mut total_latency_p95 = 0.0;
            let mut total_latency_p99 = 0.0;
            let mut valid_pods = 0;

            for pod_name in &pod_names {
                let mut cpu_usage = None;
                let mut memory_usage = None;

                // Obtener métricas de recursos de Kubernetes
                if let Some(k8s_metric) = k8s_metrics_map.get(pod_name) {
                    for container in &k8s_metric.containers {
                        if let Some(cpu_str) = &container.usage.cpu {
                            if let Some(cpu_val) = parse_cpu_millicores(cpu_str) {
                                cpu_usage = Some(cpu_usage.unwrap_or(0.0) + cpu_val);
                            }
                        }
                        if let Some(memory_str) = &container.usage.memory {
                            if let Some(memory_val) = parse_memory_bytes(memory_str) {
                                memory_usage = Some(memory_usage.unwrap_or(0.0) + memory_val);
                            }
                        }
                    }
                }

                // Obtener métricas de Istio
                let request_rate = request_rates.get(pod_name).copied();
                let latency_p50_val = latency_p50.get(pod_name).copied();
                let latency_p95_val = latency_p95.get(pod_name).copied();
                let latency_p99_val = latency_p99.get(pod_name).copied();

                let pod_metric = PodMetrics {
                    namespace: self.config.namespace.clone(),
                    pod_name: pod_name.clone(),
                    service_name: Some(service_name.clone()),
                    cpu_usage,
                    memory_usage,
                    request_rate,
                    latency_p50: latency_p50_val,
                    latency_p95: latency_p95_val,
                    latency_p99: latency_p99_val,
                    last_updated: Utc::now(),
                };

                // Acumular totales para métricas del servicio
                if let Some(cpu) = cpu_usage {
                    total_cpu += cpu;
                }
                if let Some(memory) = memory_usage {
                    total_memory += memory;
                }
                if let Some(rate) = request_rate {
                    total_request_rate += rate;
                }
                if let Some(lat) = latency_p50_val {
                    total_latency_p50 += lat;
                    valid_pods += 1;
                }
                if let Some(lat) = latency_p95_val {
                    total_latency_p95 += lat;
                }
                if let Some(lat) = latency_p99_val {
                    total_latency_p99 += lat;
                }

                pods.push(pod_metric);
            }

            // Calcular promedios
            let pod_count = pod_names.len() as f64;
            let valid_pod_count = if valid_pods > 0 { valid_pods as f64 } else { 1.0 };

            let service_metric = ServiceMetrics {
                namespace: self.config.namespace.clone(),
                service_name: service_name.clone(),
                pods,
                total_cpu,
                total_memory,
                avg_request_rate: total_request_rate / pod_count,
                avg_latency_p50: total_latency_p50 / valid_pod_count,
                avg_latency_p95: total_latency_p95 / valid_pod_count,
                avg_latency_p99: total_latency_p99 / valid_pod_count,
                last_updated: Utc::now(),
            };

            service_metrics.push(service_metric);
        }

        info!("Collected metrics for {} services", service_metrics.len());

        Ok(MetricsResponse {
            namespace: self.config.namespace.clone(),
            services: service_metrics,
            timestamp: Utc::now(),
        })
    }

    pub async fn health_check(&self) -> bool {
        // Verificar conectividad con Kubernetes
        match self.kubernetes_client.get_services().await {
            Ok(_) => {
                // Verificar conectividad con Prometheus/Istio
                match self.istio_client.health_check().await {
                    Ok(prometheus_healthy) => prometheus_healthy,
                    Err(e) => {
                        warn!("Prometheus health check failed: {}", e);
                        false
                    }
                }
            }
            Err(e) => {
                warn!("Kubernetes health check failed: {}", e);
                false
            }
        }
    }
}