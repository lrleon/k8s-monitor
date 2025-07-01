# Monitor Configuration Manual

This manual explains how to configure the Kubernetes metrics monitor using the `monitor-config.toml` configuration file.

## Overview

The alert system monitors Kubernetes services and triggers alerts when metrics exceed defined thresholds for a sustained period. The configuration allows you to:

- Define which services to monitor or exclude
- Set different alert thresholds per service
- Configure metric collection window sizes
- Customize alert notification settings

## Configuration Sections

### 1. General Settings

```toml
[general]
evaluation_interval = "1m"    # How often to check metrics against thresholds
default_namespace = "default" # Default Kubernetes namespace to monitor
enabled = true                # Global enable/disable for alerting
```

**evaluation_interval**: Controls how frequently the system evaluates all metrics against thresholds. Shorter intervals provide faster alert detection but consume more resources.

### 2. Monitoring Configuration

```toml
[monitoring]
include_services = ["user-service", "payment-service"]  # Monitor only these services
exclude_services = ["debug-service", "temp-service"]    # Never monitor these services
window_sizes = { latency = 256, request_rate = 256 }    # Metric calculation windows
```

**Service Selection Logic:**
- If `include_services` is **not empty**: Only monitor the listed services
- If `include_services` is **empty**: Monitor all services EXCEPT those in `exclude_services`
- Services with `enabled = false` in their individual config are never monitored

**window_sizes**: Number of data points used for calculating rolling averages and percentiles. Larger windows provide smoother metrics but slower response to changes.

### 3. Default Thresholds

```toml
[default_thresholds]
```

These thresholds apply to all monitored services unless overridden in service-specific configuration.

#### Memory Thresholds
```toml
[default_thresholds.memory]
enabled = true           # Enable memory monitoring
warning_percent = 70.0   # Trigger warning at 70% memory usage
critical_percent = 85.0  # Trigger critical alert at 85% memory usage
duration = "1m"          # Memory must exceed threshold for 1 minute
```

#### CPU Thresholds
```toml
[default_thresholds.cpu]
enabled = true              # Enable CPU monitoring
warning_millicores = 800.0  # Warning at 0.8 CPU cores (800 millicores)
critical_millicores = 950.0 # Critical at 0.95 CPU cores
duration = "1m"             # CPU must exceed threshold for 1 minute
```

#### Latency Thresholds
```toml
[default_thresholds.latency]
enabled = true

[default_thresholds.latency.p50]  # Median latency
warning_ms = 2000.0   # Warning at 2 seconds
critical_ms = 4000.0  # Critical at 4 seconds
duration = "1m"       # Latency must exceed threshold for 1 minute

[default_thresholds.latency.p95]  # 95th percentile latency
warning_ms = 3000.0
critical_ms = 6000.0
duration = "1m"

[default_thresholds.latency.p99]  # 99th percentile latency  
warning_ms = 4000.0
critical_ms = 8000.0
duration = "1m"
```

#### Request Rate Thresholds
```toml
[default_thresholds.request_rate]
enabled = true
min_warning_rps = 0.1     # Alert if traffic drops below 0.1 requests/second
max_warning_rps = 1000.0  # Alert if traffic exceeds 1000 requests/second
max_critical_rps = 2000.0 # Critical alert at 2000 requests/second
duration = "1m"           # Traffic must be outside bounds for 1 minute
```

#### Availability Thresholds
```toml
[default_thresholds.availability]
enabled = true
min_healthy_pods = 1         # Minimum number of healthy pods
min_healthy_percent = 80.0   # Minimum percentage of pods that must be healthy
duration = "1m"              # Pod availability must be below threshold for 1 minute
```

### 4. Service-Specific Configuration

Override default settings for individual services:

```toml
[services.user-service]
enabled = true                                    # Enable monitoring for this service
namespace = "production"                          # Override default namespace
window_sizes = { latency = 512, request_rate = 128 }  # Custom window sizes

[services.user-service.memory]
enabled = true
warning_percent = 60.0    # Stricter than default (70%)
critical_percent = 75.0   # Stricter than default (85%)
duration = "1m"

[services.user-service.cpu]
enabled = false           # Disable CPU monitoring for this service
```

### 5. Duration Field Explained

The `duration` field specifies how long a metric must **continuously** exceed a threshold before triggering an alert.

**Why duration matters:**
- **Without duration**: Momentary spikes (garbage collection, single slow request) trigger false alerts
- **With duration**: Only sustained problems trigger alerts

**Examples:**
```toml
duration = "30s"  # Fast response for critical services
duration = "5m"   # Slower response for batch/background services
```

**Duration formats:**
- `"30s"` = 30 seconds
- `"5m"` = 5 minutes
- `"2h"` = 2 hours

### 6. Notifications and Cooldown

```toml
[notifications]
enabled = true
global_cooldown = "10m"      # No alerts for ANY service during this period after an alert
per_service_cooldown = "5m"  # Specific service won't send repeat alerts during this period  
channels = ["webhook", "log", "opsgenie"]

[notifications.webhook]
enabled = true
url = "http://alertmanager.monitoring:9093/api/v1/alerts"
timeout = "10s"

[notifications.opsgenie]
enabled = true
api_key = "your-opsgenie-api-key"
region = "us"  # "us" or "eu"
timeout = "10s"

[notifications.slack]
enabled = false
webhook_url = "https://hooks.slack.com/..."
channel = "#alerts"
```

**Cooldown Behavior:**

- **global_cooldown**: After ANY alert is sent, the system won't send ANY new alerts (for any service) during this period. Prevents alert storms across the entire system.

- **per_service_cooldown**: After an alert for a specific service, that service won't send repeat alerts during this period. Allows other services to still send alerts.

**Example Timeline:**
```
10:00 - user-service memory alert sent
10:01 - payment-service latency issue detected → BLOCKED by global_cooldown (9 min left)
10:05 - user-service memory still high → BLOCKED by per_service_cooldown (still in 5min window)  
10:10 - global_cooldown expires
10:11 - payment-service latency alert sent (if still above threshold)
10:15 - user-service memory alert sent again (per_service_cooldown expired)
```

## Configuration Examples

### Example 1: Critical User-Facing Service
```toml
[services.payment-api]
enabled = true
namespace = "production"

[services.payment-api.memory]
warning_percent = 60.0
critical_percent = 75.0
duration = "30s"  # Fast alerts for critical service

[services.payment-api.latency.p99]
warning_ms = 1000.0   # Very strict latency requirements
critical_ms = 2000.0
duration = "30s"
```

### Example 2: Batch Processing Service
```toml
[services.data-processor]
enabled = true
namespace = "batch"

[services.data-processor.memory]
warning_percent = 90.0  # Higher memory tolerance
critical_percent = 95.0
duration = "5m"         # Longer duration for batch jobs

[services.data-processor.latency]
enabled = false         # Latency not relevant for batch jobs

[services.data-processor.request_rate]
enabled = false         # Request rate not relevant for batch jobs
```

### Example 3: Infrastructure Service
```toml
[services.redis-cache]
enabled = true

[services.redis-cache.memory]
warning_percent = 80.0
critical_percent = 90.0
duration = "2m"

[services.redis-cache.cpu]
enabled = false  # CPU monitoring not needed for cache

[services.redis-cache.latency]
enabled = true
[services.redis-cache.latency.p50]
warning_ms = 5.0     # Very low latency requirements for cache
critical_ms = 25.0
duration = "1m"

[services.redis-cache.latency.p95]
warning_ms = 50.0
critical_ms = 100.0
duration = "1m"

[services.redis-cache.latency.p99]
warning_ms = 100.0
critical_ms = 200.0
duration = "1m"
```

### Example 4: Service to Exclude
```toml
[services.debug-tool]
enabled = false  # Never monitor this service
```

## Best Practices

### 1. Service Selection
- Use `include_services` for environments with many services where you only want to monitor a few
- Use `exclude_services` for environments where you want to monitor most services except specific ones
- Don't use both `include_services` and `exclude_services` simultaneously

### 2. Threshold Setting
- **Production services**: Stricter thresholds (60-75% memory, 1-2s latency)
- **Development services**: Looser thresholds (80-90% memory, 5-10s latency)
- **Batch/Background services**: Very loose thresholds, longer durations

### 3. Duration Selection
- **Critical services**: 30s-1m for fast response
- **Normal services**: 1-2m for balanced alerting
- **Batch services**: 5-10m to avoid false alerts during normal processing

### 4. Window Sizes
- **Latency**: Larger windows (256-512) for smoother latency calculations
- **Request Rate**: Smaller windows (64-128) for faster traffic change detection
- **High-traffic services**: Larger windows to reduce noise
- **Low-traffic services**: Smaller windows for faster detection

## Configuration Validation

The system validates your configuration and will:
- Use defaults if `monitor-config.toml` is missing
- Log warnings for invalid duration formats
- Ignore unknown configuration fields
- Apply service overrides on top of defaults

## Troubleshooting

### No Alerts Being Generated
1. Check `general.enabled = true`
2. Verify service is not in `exclude_services`
3. Check service-specific `enabled = true`
4. Verify thresholds are actually being exceeded for the specified `duration`

### Too Many False Alerts
1. Increase `duration` values
2. Increase threshold percentages/values
3. Increase `window_sizes` for smoother metrics
4. Increase `notifications.global_cooldown` or `notifications.per_service_cooldown`

### Missing Service Alerts
1. Add service to `include_services` if using include mode
2. Remove service from `exclude_services`
3. Set `[services.service-name] enabled = true`