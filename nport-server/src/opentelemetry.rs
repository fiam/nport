use std::collections::HashMap;
use std::time::Duration;

use opentelemetry::metrics::MeterProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::metrics::reader::{DefaultAggregationSelector, DefaultTemporalitySelector};
use opentelemetry_sdk::trace::{self, RandomIdGenerator, Sampler};
use opentelemetry_sdk::Resource;

use tracing::{subscriber::Interest, Metadata};
use tracing_subscriber::layer;

pub struct Filter;

impl Filter {
    fn is_enabled(&self, metadata: &Metadata<'_>) -> bool {
        !metadata.target().starts_with("h2::") && !metadata.target().starts_with("hyper::proto")
    }
}

impl<S> layer::Filter<S> for Filter {
    fn enabled(&self, metadata: &Metadata<'_>, _: &layer::Context<'_, S>) -> bool {
        self.is_enabled(metadata)
    }

    fn callsite_enabled(&self, metadata: &'static Metadata<'static>) -> Interest {
        if self.is_enabled(metadata) {
            Interest::always()
        } else {
            Interest::never()
        }
    }
}

pub fn filter() -> Filter {
    Filter {}
}

pub struct OpenTelemetry {
    pub tracer: trace::Tracer,
    pub meter: opentelemetry::metrics::Meter,
}

pub fn new(
    service_name: &str,
    endpoint: &str,
    headers: HashMap<String, String>,
) -> anyhow::Result<OpenTelemetry> {
    let mut metadata = tonic::metadata::MetadataMap::new();
    headers.iter().for_each(|(k, v)| {
        let k = tonic::metadata::MetadataKey::from_bytes(k.as_bytes()).unwrap();
        metadata.insert(k, v.parse().unwrap());
    });
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint)
                .with_metadata(metadata.clone())
                .with_timeout(std::time::Duration::from_secs(3)),
        )
        .with_trace_config(
            trace::config()
                .with_sampler(Sampler::AlwaysOn)
                .with_id_generator(RandomIdGenerator::default())
                .with_max_events_per_span(64)
                .with_max_attributes_per_span(16)
                .with_max_events_per_span(16)
                .with_resource(Resource::new(vec![KeyValue::new(
                    "service.name",
                    service_name.to_string(),
                )])),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    let meter_provider = opentelemetry_otlp::new_pipeline()
        .metrics(opentelemetry_sdk::runtime::Tokio)
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint)
                .with_metadata(metadata.clone())
                .with_timeout(std::time::Duration::from_secs(3)),
        )
        .with_resource(Resource::new(vec![KeyValue::new(
            "service.name",
            service_name.to_string(),
        )]))
        .with_period(Duration::from_secs(3))
        .with_timeout(Duration::from_secs(10))
        .with_aggregation_selector(DefaultAggregationSelector::new())
        .with_temporality_selector(DefaultTemporalitySelector::new())
        .build()?;

    Ok(OpenTelemetry {
        tracer,
        meter: meter_provider.meter(service_name.to_string()),
    })
}
