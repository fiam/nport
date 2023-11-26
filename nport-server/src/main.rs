use std::collections::HashMap;

use dotenvy::dotenv;
use envconfig::Envconfig;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::Layer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod opentelemetry;

#[tokio::main]
async fn main() {
    dotenv().ok();

    let config = nport_server::server::Config::init_from_env().unwrap();

    let opentelemetry_headers = config
        .otel_otlp_headers()
        .split(',')
        .map(|header| {
            let mut parts = header.split('=');

            let key = parts.next().unwrap().to_string();
            let value = parts.next().expect("invalid header format").to_string();
            (key, value)
        })
        .collect::<HashMap<String, String>>();

    let telemetry = opentelemetry::new(
        config.otel_service_name(),
        config.otel_otlp_endpoint(),
        opentelemetry_headers,
    )
    .unwrap();

    let opentelemetry_layer = tracing_opentelemetry::layer()
        .with_tracer(telemetry.tracer)
        .with_filter(opentelemetry::filter());

    let registry = tracing_subscriber::registry();

    let env_filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "WARN".into());

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_filter(env_filter);

    registry.with(opentelemetry_layer).with(fmt_layer).init();

    let stats = nport_server::server::Stats::new(&telemetry.meter);
    let server = config
        .server(Some(stats))
        .await
        .expect("invalid Server configuration");
    server.run().await
}
