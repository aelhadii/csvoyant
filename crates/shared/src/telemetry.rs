//! OpenTelemetry + tracing setup shared by both binaries.
//!
//! Always installs a stdout (JSON) log layer driven by `RUST_LOG`. When an OTLP endpoint is
//! configured (see [`TelemetryConfig`]), it additionally exports **traces** and **logs** to the
//! collector / Axiom over OTLP-HTTP. Metrics can be layered on the same exporter pattern later.

use std::collections::HashMap;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, SpanExporter, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::config::TelemetryConfig;

/// Held for the lifetime of the process; flushes exporters on drop.
pub struct TelemetryGuard {
    tracer_provider: Option<SdkTracerProvider>,
    logger_provider: Option<SdkLoggerProvider>,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(tp) = self.tracer_provider.take() {
            let _ = tp.shutdown();
        }
        if let Some(lp) = self.logger_provider.take() {
            let _ = lp.shutdown();
        }
    }
}

/// Build the Axiom / OTLP header set (empty when not targeting Axiom).
fn otlp_headers(cfg: &TelemetryConfig) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    if !cfg.axiom_token.is_empty() {
        headers.insert(
            "Authorization".to_string(),
            format!("Bearer {}", cfg.axiom_token),
        );
    }
    if !cfg.axiom_dataset.is_empty() {
        headers.insert("X-Axiom-Dataset".to_string(), cfg.axiom_dataset.clone());
    }
    headers
}

fn resource(service_name: &str) -> Resource {
    Resource::builder()
        .with_service_name(service_name.to_string())
        .build()
}

/// Initialize telemetry for a service. Returns a guard that must be kept alive for the whole run.
pub fn init(service_name: &'static str, cfg: &TelemetryConfig) -> anyhow::Result<TelemetryGuard> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer().json().with_target(true);

    if !cfg.export_enabled() {
        // No OTLP endpoint: stdout logging only. Still a fully healthy service.
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
        tracing::info!(
            service = service_name,
            "telemetry initialized (stdout only)"
        );
        return Ok(TelemetryGuard {
            tracer_provider: None,
            logger_provider: None,
        });
    }

    let base = cfg.otlp_endpoint.trim_end_matches('/');
    let headers = otlp_headers(cfg);

    // Traces
    let span_exporter = SpanExporter::builder()
        .with_http()
        .with_endpoint(format!("{base}/v1/traces"))
        .with_headers(headers.clone())
        .build()?;
    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(resource(service_name))
        .with_batch_exporter(span_exporter)
        .build();
    let tracer = tracer_provider.tracer(service_name);
    let otel_trace_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // Logs
    let log_exporter = LogExporter::builder()
        .with_http()
        .with_endpoint(format!("{base}/v1/logs"))
        .with_headers(headers)
        .build()?;
    let logger_provider = SdkLoggerProvider::builder()
        .with_resource(resource(service_name))
        .with_batch_exporter(log_exporter)
        .build();
    let otel_log_layer = OpenTelemetryTracingBridge::new(&logger_provider);

    opentelemetry::global::set_tracer_provider(tracer_provider.clone());

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(otel_trace_layer)
        .with(otel_log_layer.boxed())
        .init();

    tracing::info!(
        service = service_name,
        endpoint = base,
        "telemetry initialized (OTLP export enabled)"
    );

    Ok(TelemetryGuard {
        tracer_provider: Some(tracer_provider),
        logger_provider: Some(logger_provider),
    })
}
