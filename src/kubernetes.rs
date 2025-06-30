use anyhow::{Context, Result};
use k8s_openapi::api::core::v1::{Pod, Service};
use kube::{
    api::{Api, ListParams},
    Client,
};
use std::collections::HashMap;
use tracing::{debug, error, info};

use crate::types::{ContainerMetric, KubernetesMetric, MetricMetadata, ResourceUsage};

pub struct KubernetesClient {
    client: Client,
    namespace: String,
}

impl KubernetesClient {
    pub async fn new(namespace: String) -> Result<Self> {
        let client = Client::try_default()
            .await
            .context("Failed to create Kubernetes client")?;

        info!("Connected to Kubernetes cluster");

        Ok(Self { client, namespace })
    }

    pub async fn get_services(&self) -> Result<Vec<Service>> {
        let services: Api<Service> = Api::namespaced(self.client.clone(), &self.namespace);
        let lp = ListParams::default();

        let service_list = services
            .list(&lp)
            .await
            .context("Failed to list services")?;

        debug!("Found {} services in namespace {}", service_list.items.len(), self.namespace);
        Ok(service_list.items)
    }

    pub async fn get_pods(&self) -> Result<Vec<Pod>> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);
        let lp = ListParams::default();

        let pod_list = pods
            .list(&lp)
            .await
            .context("Failed to list pods")?;

        debug!("Found {} pods in namespace {}", pod_list.items.len(), self.namespace);
        Ok(pod_list.items)
    }

    pub async fn get_pod_metrics(&self) -> Result<Vec<KubernetesMetric>> {
        //Note: This requires that the Metrics-Server be installed in the cluster
        let metrics_url = format!("/apis/metrics.k8s.io/v1beta1/namespaces/{}/pods", self.namespace);

        let response = self.client
            .request_text(http::Request::builder()
                .uri(&metrics_url)
                .body(vec![])?)
            .await
            .context("Failed to fetch pod metrics")?;

        let metrics_response: serde_json::Value = serde_json::from_str(&response)
            .context("Failed to parse metrics response")?;

        let mut metrics = Vec::new();

        if let Some(items) = metrics_response.get("items").and_then(|v| v.as_array()) {
            for item in items {
                if let Ok(metric) = self.parse_pod_metric(item) {
                    metrics.push(metric);
                }
            }
        }

        debug!("Retrieved metrics for {} pods", metrics.len());
        Ok(metrics)
    }

    fn parse_pod_metric(&self, item: &serde_json::Value) -> Result<KubernetesMetric> {
        let metadata = item.get("metadata")
            .context("Missing metadata in pod metric")?;

        let name = metadata.get("name")
            .and_then(|v| v.as_str())
            .context("Missing pod name")?
            .to_string();

        let namespace = metadata.get("namespace")
            .and_then(|v| v.as_str())
            .context("Missing namespace")?
            .to_string();

        let containers_data = item.get("containers")
            .and_then(|v| v.as_array())
            .context("Missing containers in pod metric")?;

        let mut containers = Vec::new();
        for container_data in containers_data {
            let container_name = container_data.get("name")
                .and_then(|v| v.as_str())
                .context("Missing container name")?
                .to_string();

            let usage_data = container_data.get("usage");
            let cpu = usage_data
                .and_then(|u| u.get("cpu"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let memory = usage_data
                .and_then(|u| u.get("memory"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            containers.push(ContainerMetric {
                name: container_name,
                usage: ResourceUsage { cpu, memory },
            });
        }

        Ok(KubernetesMetric {
            metadata: MetricMetadata { name, namespace },
            containers,
        })
    }

    pub async fn get_services_with_pods(&self) -> Result<HashMap<String, Vec<String>>> {
        let services = self.get_services().await?;
        let pods = self.get_pods().await?;

        let mut service_pods: HashMap<String, Vec<String>> = HashMap::new();

        for service in services {
            let service_name = service.metadata.name.clone().unwrap_or_default();
            let selector = service.spec
                .as_ref()
                .and_then(|s| s.selector.as_ref());

            if let Some(selector) = selector {
                let matching_pods: Vec<String> = pods
                    .iter()
                    .filter(|pod| {
                        if let Some(pod_labels) = &pod.metadata.labels {
                            selector.iter().all(|(key, value)| {
                                pod_labels.get(key).map_or(false, |v| v == value)
                            })
                        } else {
                            false
                        }
                    })
                    .filter_map(|pod| pod.metadata.name.clone())
                    .collect();

                service_pods.insert(service_name, matching_pods);
            }
        }

        Ok(service_pods)
    }
}