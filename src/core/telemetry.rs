use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_telemetry() -> PrometheusHandle {
    // Setup Prometheus metrics recorder
    let builder = PrometheusBuilder::new();
    let handle = builder
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    // Setup structured logging filter
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,api=debug,tower_http=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    handle
}
