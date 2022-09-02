use std::{collections::HashMap, io};

use anyhow::{Context, Result};
use opentelemetry::{
    global,
    propagation::TextMapPropagator,
    sdk::{
        propagation::{BaggagePropagator, TextMapCompositePropagator, TraceContextPropagator},
        trace as sdktrace, Resource,
    },
    trace::TraceError,
    KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::{
    fmt::{format::FmtSpan, time::UtcTime},
    prelude::__tracing_subscriber_SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};
use url::Url;

const ENDPOINT: &str = "OTLP_ENDPOINT";
const HEADER_PREFIX: &str = "OTLP_";

pub(crate) fn init() -> Result<()> {
    let propagator = new_propagator();
    global::set_text_map_propagator(propagator);

    let tracer = new_tracer().context("Failed to create tracer")?;

    let fmt_env_filter = EnvFilter::builder()
        .with_default_directive(
            "info"
                .parse()
                .context("Failed to parse default EnvFilter directives")?,
        )
        .with_env_var("RUSTY_PEANUTS_LOG_LEVEL")
        .from_env_lossy();
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(io::stderr)
        .with_timer(UtcTime::rfc_3339())
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .with_filter(fmt_env_filter);

    let otel_env_filter = EnvFilter::builder()
        .with_default_directive(
            "trace,polling=info"
                .parse()
                .context("Failed to parse default EnvFilter directives")?,
        )
        .with_env_var("RUSTY_PEANUTS_TRACE_LEVEL")
        .from_env_lossy();
    let otel_layer = tracing_opentelemetry::layer()
        .with_tracer(tracer)
        .with_filter(otel_env_filter);

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(otel_layer)
        .try_init()
        .context("Failed to set global default tracing subscriber")?;

    Ok(())
}

fn new_propagator() -> impl TextMapPropagator {
    let bagage_propagator = BaggagePropagator::new();
    let trace_context_propagator = TraceContextPropagator::new();

    TextMapCompositePropagator::new(vec![
        Box::new(bagage_propagator),
        Box::new(trace_context_propagator),
    ])
}

fn new_tracer() -> Result<sdktrace::Tracer, TraceError> {
    let endpoint = std::env::var(ENDPOINT).unwrap();
    let endpoint = Url::parse(&endpoint).unwrap();
    std::env::remove_var(ENDPOINT);

    let headers: HashMap<_, _> = std::env::vars()
        .filter(|(name, _)| name.starts_with(HEADER_PREFIX))
        .map(|(name, value)| {
            let header_name = name
                .strip_prefix(HEADER_PREFIX)
                .unwrap()
                .replace('_', "-")
                .to_ascii_lowercase();
            (header_name, value)
        })
        .collect();

    let endpoint = format!(
        "{}:{}",
        endpoint.host_str().unwrap(),
        endpoint.port_or_known_default().unwrap()
    );

    let exporter = opentelemetry_otlp::new_exporter()
        .grpcio()
        .with_endpoint(endpoint)
        .with_headers(headers)
        .with_tls(true);

    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(
            sdktrace::config().with_resource(Resource::new(vec![KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                "rusty-peanuts",
            )])),
        )
        .install_batch(opentelemetry::runtime::AsyncStd)
}
