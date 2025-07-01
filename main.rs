use k8s_metrics_monitor::{MonitorConfig, ConfigError};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::init();

    // Load configuration
    let config = MonitorConfig::load_from_file("monitor-config.toml")?;
    tracing::info!("Configuration loaded successfully");

    // Example: Print monitored services
    let monitored_services = config.get_monitored_services();
    tracing::info!("Services to monitor: {:?}", monitored_services);

    // Your existing code here...

    Ok(())
}