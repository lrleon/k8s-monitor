use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::types::{
    Alert, AlertConditions, AlertStatus, AlertThresholds, AlertType, Config, PodMetrics,
    TimeSeriesData, TimestampedValue,
};

pub struct AlertManager {
    client: Client,
    config: Config,
    active_alerts: Arc<RwLock<HashMap<String, Alert>>>,
    time_series_data: Arc<RwLock<HashMap<String, TimeSeriesData>>>,
}

impl AlertManager {
    pub fn new(config: Config) -> Self {
        Self {
            client: Client::new(),
            config,
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
            time_series_data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn process_metrics(&self, pods: &[PodMetrics]) {
        if !self.config.alerting.enabled {
            return;
        }

        // Update time series data for each pod
        let mut ts_data = self.time_series_data.write().await;

        for pod in pods {
            let pod_key = format!("{}:{}", pod.namespace, pod.pod_name);

            let pod_ts = ts_data.entry(pod_key.clone()).or_insert_with(TimeSeriesData::new);
            pod_ts.add_sample(pod, self.config.metrics.window_size);

            // Check for threshold violations
            self.check_memory_threshold(pod, &pod_ts.memory_usage).await;
            self.check_latency_thresholds(pod, &pod_ts.latency_p95, &pod_ts.latency_p99).await;
        }

        // Clean up old time series data (optional optimization)
        self.cleanup_old_data(&mut ts_data).await;
    }

    async fn check_memory_threshold(&self, pod: &PodMetrics, memory_data: &std::collections::VecDeque<TimestampedValue>) {
        if let Some(current_memory) = pod.memory_usage {
            if current_memory > self.config.alerting.thresholds.memory_threshold_bytes {
                let duration_exceeded = self.calculate_threshold_duration(
                    memory_data,
                    self.config.alerting.thresholds.memory_threshold_bytes,
                );

                if duration_exceeded >= self.config.alerting.conditions.memory_duration_seconds as i64 {
                    let alert_id = format!("memory_{}_{}", pod.namespace, pod.pod_name);

                    if !self.is_alert_in_cooldown(&alert_id).await {
                        let alert = Alert {
                            id: alert_id.clone(),
                            pod_name: pod.pod_name.clone(),
                            namespace: pod.namespace.clone(),
                            service_name: pod.service_name.clone(),
                            alert_type: AlertType::MemoryThreshold,
                            message: format!(
                                "Memory usage {} bytes exceeds threshold {} bytes for pod {} in namespace {}",
                                current_memory,
                                self.config.alerting.thresholds.memory_threshold_bytes,
                                pod.pod_name,
                                pod.namespace
                            ),
                            value: current_memory,
                            threshold: self.config.alerting.thresholds.memory_threshold_bytes,
                            first_seen: pod.last_updated,
                            last_seen: pod.last_updated,
                            status: AlertStatus::Active,
                        };

                        self.trigger_alert(alert).await;
                    }
                }
            } else {
                // Check if we need to resolve an existing alert
                let alert_id = format!("memory_{}_{}", pod.namespace, pod.pod_name);
                self.resolve_alert(&alert_id).await;
            }
        }
    }

    async fn check_latency_thresholds(
        &self,
        pod: &PodMetrics,
        latency_p95_data: &std::collections::VecDeque<TimestampedValue>,
        latency_p99_data: &std::collections::VecDeque<TimestampedValue>,
    ) {
        // Check P95 latency
        if let Some(current_latency_p95) = pod.latency_p95 {
            if current_latency_p95 > self.config.alerting.thresholds.latency_p95_threshold_ms {
                let duration_exceeded = self.calculate_threshold_duration(
                    latency_p95_data,
                    self.config.alerting.thresholds.latency_p95_threshold_ms,
                );

                if duration_exceeded >= self.config.alerting.conditions.latency_duration_seconds as i64 {
                    let alert_id = format!("latency_p95_{}_{}", pod.namespace, pod.pod_name);

                    if !self.is_alert_in_cooldown(&alert_id).await {
                        let alert = Alert {
                            id: alert_id.clone(),
                            pod_name: pod.pod_name.clone(),
                            namespace: pod.namespace.clone(),
                            service_name: pod.service_name.clone(),
                            alert_type: AlertType::LatencyP95Threshold,
                            message: format!(
                                "P95 latency {} ms exceeds threshold {} ms for pod {} in namespace {}",
                                current_latency_p95,
                                self.config.alerting.thresholds.latency_p95_threshold_ms,
                                pod.pod_name,
                                pod.namespace
                            ),
                            value: current_latency_p95,
                            threshold: self.config.alerting.thresholds.latency_p95_threshold_ms,
                            first_seen: pod.last_updated,
                            last_seen: pod.last_updated,
                            status: AlertStatus::Active,
                        };

                        self.trigger_alert(alert).await;
                    }
                }
            } else {
                let alert_id = format!("latency_p95_{}_{}", pod.namespace, pod.pod_name);
                self.resolve_alert(&alert_id).await;
            }
        }

        // Check P99 latency
        if let Some(current_latency_p99) = pod.latency_p99 {
            if current_latency_p99 > self.config.alerting.thresholds.latency_p99_threshold_ms {
                let duration_exceeded = self.calculate_threshold_duration(
                    latency_p99_data,
                    self.config.alerting.thresholds.latency_p99_threshold_ms,
                );

                if duration_exceeded >= self.config.alerting.conditions.latency_duration_seconds as i64 {
                    let alert_id = format!("latency_p99_{}_{}", pod.namespace, pod.pod_name);

                    if !self.is_alert_in_cooldown(&alert_id).await {
                        let alert = Alert {
                            id: alert_id.clone(),
                            pod_name: pod.pod_name.clone(),
                            namespace: pod.namespace.clone(),
                            service_name: pod.service_name.clone(),
                            alert_type: AlertType::LatencyP99Threshold,
                            message: format!(
                                "P99 latency {} ms exceeds threshold {} ms for pod {} in namespace {}",
                                current_latency_p99,
                                self.config.alerting.thresholds.latency_p99_threshold_ms,
                                pod.pod_name,
                                pod.namespace
                            ),
                            value: current_latency_p99,
                            threshold: self.config.alerting.thresholds.latency_p99_threshold_ms,
                            first_seen: pod.last_updated,
                            last_seen: pod.last_updated,
                            status: AlertStatus::Active,
                        };

                        self.trigger_alert(alert).await;
                    }
                }
            } else {
                let alert_id = format!("latency_p99_{}_{}", pod.namespace, pod.pod_name);
                self.resolve_alert(&alert_id).await;
            }
        }
    }

    fn calculate_threshold_duration(
        &self,
        data: &std::collections::VecDeque<TimestampedValue>,
        threshold: f64,
    ) -> i64 {
        if data.is_empty() {
            return 0;
        }

        let now = Utc::now();
        let mut consecutive_duration = 0i64;

        // Check from most recent backwards
        for sample in data.iter().rev() {
            if sample.value > threshold {
                let duration = (now - sample.timestamp).num_seconds();
                consecutive_duration = consecutive_duration.max(duration);
            } else {
                break; // Stop at first non-violating sample
            }
        }

        consecutive_duration
    }

    async fn is_alert_in_cooldown(&self, alert_id: &str) -> bool {
        let alerts = self.active_alerts.read().await;

        if let Some(alert) = alerts.get(alert_id) {
            let cooldown_period = chrono::Duration::seconds(
                self.config.alerting.conditions.alert_cooldown_seconds as i64
            );

            (Utc::now() - alert.last_seen) < cooldown_period
        } else {
            false
        }
    }

    async fn trigger_alert(&self, mut alert: Alert) {
        info!("Triggering alert: {}", alert.message);

        // Check if alert already exists and update it
        let mut alerts = self.active_alerts.write().await;
        if let Some(existing_alert) = alerts.get(&alert.id) {
            alert.first_seen = existing_alert.first_seen;
        }

        // Send to OpsGenie
        if let Err(e) = self.send_to_opsgenie(&alert).await {
            error!("Failed to send alert to OpsGenie: {}", e);
        }

        // Store the alert
        alerts.insert(alert.id.clone(), alert);
    }

    async fn resolve_alert(&self, alert_id: &str) {
        let mut alerts = self.active_alerts.write().await;

        if let Some(mut alert) = alerts.remove(alert_id) {
            alert.status = AlertStatus::Resolved;
            alert.last_seen = Utc::now();

            info!("Resolving alert: {}", alert.message);

            // Send resolution to OpsGenie
            if let Err(e) = self.send_resolution_to_opsgenie(&alert).await {
                error!("Failed to send alert resolution to OpsGenie: {}", e);
            }
        }
    }

    async fn send_to_opsgenie(&self, alert: &Alert) -> Result<()> {
        if self.config.alerting.opsgenie.api_key.is_empty() {
            warn!("OpsGenie API key not configured, skipping alert");
            return Ok(());
        }

        let url = format!("{}/v2/alerts", self.config.alerting.opsgenie.api_url);

        let payload = json!({
            "message": alert.message,
            "alias": alert.id,
            "description": format!(
                "Pod: {}\nNamespace: {}\nService: {}\nCurrent Value: {}\nThreshold: {}\nFirst Seen: {}",
                alert.pod_name,
                alert.namespace,
                alert.service_name.as_deref().unwrap_or("N/A"),
                alert.value,
                alert.threshold,
                alert.first_seen
            ),
            "teams": [{"name": self.config.alerting.opsgenie.team}],
            "priority": self.config.alerting.opsgenie.priority,
            "tags": [
                format!("namespace:{}", alert.namespace),
                format!("pod:{}", alert.pod_name),
                format!("alert_type:{:?}", alert.alert_type),
                "k8s-metrics-monitor"
            ],
            "details": {
                "namespace": alert.namespace,
                "pod_name": alert.pod_name,
                "service_name": alert.service_name,
                "value": alert.value,
                "threshold": alert.threshold,
                "alert_type": format!("{:?}", alert.alert_type)
            }
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("GenieKey {}", self.config.alerting.opsgenie.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .context("Failed to send request to OpsGenie")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("OpsGenie resolution API error {}: {}", status, body));
        }

        debug!("Successfully sent alert resolution to OpsGenie: {}", alert.id);
        Ok(())
    }

    async fn cleanup_old_data(&self, ts_data: &mut HashMap<String, TimeSeriesData>) {
        let retention_duration = chrono::Duration::minutes(
            self.config.metrics.latency_window_minutes.max(
                self.config.metrics.request_rate_window_minutes
            ) as i64
        );
        let cutoff_time = Utc::now() - retention_duration;

        for (_, data) in ts_data.iter_mut() {
            // Remove old samples
            data.request_rates.retain(|sample| sample.timestamp > cutoff_time);
            data.latency_p50.retain(|sample| sample.timestamp > cutoff_time);
            data.latency_p95.retain(|sample| sample.timestamp > cutoff_time);
            data.latency_p99.retain(|sample| sample.timestamp > cutoff_time);
            data.memory_usage.retain(|sample| sample.timestamp > cutoff_time);
            data.cpu_usage.retain(|sample| sample.timestamp > cutoff_time);
        }

        // Remove pods that have no recent data
        ts_data.retain(|_, data| {
            !data.request_rates.is_empty()
                || !data.latency_p95.is_empty()
                || !data.memory_usage.is_empty()
        });
    }

    pub async fn get_active_alerts(&self) -> HashMap<String, Alert> {
        self.active_alerts.read().await.clone()
    }

    pub async fn get_time_series_data(&self, pod_key: &str) -> Option<TimeSeriesData> {
        self.time_series_data.read().await.get(pod_key).cloned()
    }

    pub async fn health_check(&self) -> bool {
        if !self.config.alerting.enabled || self.config.alerting.opsgenie.api_key.is_empty() {
            return true; // Consider healthy if alerting is disabled
        }

        // Test OpsGenie connectivity
        let url = format!("{}/v2/account", self.config.alerting.opsgenie.api_url);

        match self
            .client
            .get(&url)
            .header("Authorization", format!("GenieKey {}", self.config.alerting.opsgenie.api_key))
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(e) => {
                warn!("OpsGenie health check failed: {}", e);
                false
            }
        }
    }
} = response.text().await.unwrap_or_default();
return Err(anyhow::anyhow!("OpsGenie API error {}: {}", status, body));
}

debug!("Successfully sent alert to OpsGenie: {}", alert.id);
Ok(())
}

async fn send_resolution_to_opsgenie(&self, alert: &Alert) -> Result<()> {
    if self.config.alerting.opsgenie.api_key.is_empty() {
        return Ok(());
    }

    let url = format!("{}/v2/alerts/{}/close", self.config.alerting.opsgenie.api_url, alert.id);

    let payload = json!({
            "note": format!("Alert automatically resolved - metrics returned to normal levels at {}", alert.last_seen)
        });

    let response = self
        .client
        .post(&url)
        .header("Authorization", format!("GenieKey {}", self.config.alerting.opsgenie.api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .context("Failed to send resolution to OpsGenie")?;

    if !response.status().is_success() {
        let status = response.status();
        let body