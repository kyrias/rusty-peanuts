use std::{collections::HashMap, io};

use anyhow::{anyhow, Context, Result};
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

    let fmt_env_filter = env_filter_merge_from_environment("info", "RUSTY_PEANUTS_LOG_LEVEL")?;
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(io::stderr)
        .with_timer(UtcTime::rfc_3339())
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .with_filter(fmt_env_filter);

    let otel_env_filter =
        env_filter_merge_from_environment("trace,polling=off", "RUSTY_PEANUTS_TRACE_LEVEL")?;
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

fn env_filter_merge_from_environment(
    default_directives: &'static str,
    env_var: &'static str,
) -> Result<EnvFilter> {
    let mut filter = EnvFilter::builder()
        .parse(default_directives)
        .with_context(|| anyhow!("Default directives were invalid: {default_directives}"))?;

    if let Ok(env_value) = std::env::var(env_var) {
        for env_directive in env_value.split(',') {
            match env_directive.parse() {
                Ok(directive) => filter = filter.add_directive(directive),
                Err(err) => eprintln!("WARN ignoring log directive: {env_directive:?}: {err}"),
            }
        }
    }

    Ok(filter)
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
