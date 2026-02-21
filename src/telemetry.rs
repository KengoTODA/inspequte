use anyhow::{Context, Result, anyhow};
use opentelemetry::trace::{Span, TraceContextExt, Tracer, TracerProvider as OtelTracerProvider};
use opentelemetry::{Context as OtelContext, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, SdkTracerProvider, SpanExporter,
};
use std::time::Duration;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Telemetry handle for OpenTelemetry tracing.
pub(crate) struct Telemetry {
    tracer: opentelemetry_sdk::trace::SdkTracer,
    provider: SdkTracerProvider,
}

impl Telemetry {
    /// Initialize telemetry with an OTLP HTTP exporter.
    pub(crate) fn new(endpoint: String) -> Result<Self> {
        let endpoint = normalize_otlp_http_trace_endpoint(&endpoint)?;
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .build()
            .context("build OTLP span exporter")?;
        Self::from_exporter(exporter)
    }

    /// Run a closure inside a span when telemetry is enabled.
    pub(crate) fn in_span<T, F>(&self, name: &str, attributes: &[KeyValue], f: F) -> T
    where
        F: FnOnce() -> T,
    {
        self.tracer.in_span(name.to_string(), |cx| {
            let span = cx.span();
            for attribute in attributes {
                span.set_attribute(attribute.clone());
            }
            f()
        })
    }

    /// Run a closure inside a span, using the provided parent context.
    pub(crate) fn in_span_with_parent<T, F>(
        &self,
        name: &str,
        attributes: &[KeyValue],
        parent_cx: &OtelContext,
        f: F,
    ) -> T
    where
        F: FnOnce() -> T,
    {
        let mut span = self.tracer.start_with_context(name.to_string(), parent_cx);
        for attribute in attributes {
            span.set_attribute(attribute.clone());
        }
        let cx = parent_cx.with_span(span);
        let _guard = cx.attach();
        f()
    }

    /// Flush spans and shut down the tracer provider.
    pub(crate) fn shutdown(&self) -> Result<()> {
        if let Err(err) = self.provider.shutdown() {
            return Err(anyhow!("failed to shutdown tracer provider: {err}"));
        }
        Ok(())
    }

    fn from_exporter<E: SpanExporter + 'static>(exporter: E) -> Result<Self> {
        let resource = Resource::builder().with_service_name("inspequte").build();
        // BatchSpanProcessor in opentelemetry-sdk 0.31 uses std::thread::spawn
        // and std::sync::mpsc channels internally, so on_end() is a plain
        // channel send that works safely from rayon worker threads. The
        // background export thread calls futures_executor::block_on, which is
        // compatible with reqwest-blocking-client.
        let batch_config = BatchConfigBuilder::default()
            .with_max_queue_size(65_536)
            .with_max_export_batch_size(4096)
            .with_scheduled_delay(Duration::from_millis(200))
            .build();
        let processor = BatchSpanProcessor::builder(exporter)
            .with_batch_config(batch_config)
            .build();
        let provider = SdkTracerProvider::builder()
            .with_resource(resource)
            .with_span_processor(processor)
            .build();
        let tracer = provider.tracer("inspequte");
        opentelemetry::global::set_tracer_provider(provider.clone());
        Ok(Self { tracer, provider })
    }
}

fn normalize_otlp_http_trace_endpoint(endpoint: &str) -> Result<String> {
    let mut url = reqwest::Url::parse(endpoint).context("parse OTLP endpoint")?;
    let path = url.path().to_string();
    if path == "/" {
        url.set_path("/v1/traces");
    } else if let Some(prefix) = path.strip_suffix("/v1/logs") {
        url.set_path(&format!("{prefix}/v1/traces"));
    }
    Ok(url.to_string())
}

/// Initialize logging facade with stderr output.
pub(crate) fn init_logging() {
    let init_result = tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("inspequte=info,warn")),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .try_init();
    let _ = init_result;
}

/// Add an OpenTelemetry event to the currently active span.
pub(crate) fn add_current_span_event(name: &str, attributes: &[KeyValue]) {
    let cx = OtelContext::current();
    let span = cx.span();
    if !span.span_context().is_valid() {
        return;
    }
    span.add_event(name.to_string(), attributes.to_vec());
}

/// Return the trace ID of the current span context when available.
pub(crate) fn current_trace_id() -> Option<String> {
    let cx = OtelContext::current();
    let span = cx.span();
    let span_context = span.span_context();
    if !span_context.is_valid() {
        return None;
    }
    Some(span_context.trace_id().to_string())
}

/// Optional telemetry span helper.
pub(crate) fn with_span<T, F>(
    telemetry: Option<&Telemetry>,
    name: &str,
    attributes: &[KeyValue],
    f: F,
) -> T
where
    F: FnOnce() -> T,
{
    match telemetry {
        Some(telemetry) => telemetry.in_span(name, attributes, f),
        None => f(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_sdk::error::OTelSdkResult;
    use opentelemetry_sdk::trace::{SpanData, SpanExporter};

    #[derive(Debug)]
    struct NoopExporter;

    impl SpanExporter for NoopExporter {
        async fn export(&self, _batch: Vec<SpanData>) -> OTelSdkResult {
            Ok(())
        }
    }

    #[test]
    fn telemetry_uses_exporter_without_errors() {
        let telemetry = Telemetry::from_exporter(NoopExporter).expect("telemetry");
        telemetry.in_span("test", &[KeyValue::new("test.key", "value")], || {});
        telemetry.shutdown().expect("shutdown");
    }

    #[test]
    fn normalize_root_endpoint_to_trace_path() {
        let endpoint =
            normalize_otlp_http_trace_endpoint("http://localhost:4318/").expect("endpoint");
        assert_eq!(endpoint, "http://localhost:4318/v1/traces");
    }

    #[test]
    fn normalize_log_endpoint_to_trace_path() {
        let endpoint =
            normalize_otlp_http_trace_endpoint("http://localhost:4318/v1/logs").expect("endpoint");
        assert_eq!(endpoint, "http://localhost:4318/v1/traces");
    }

    #[test]
    fn current_trace_id_is_available_inside_span() {
        let telemetry = Telemetry::from_exporter(NoopExporter).expect("telemetry");
        let trace_id = telemetry.in_span("test", &[], current_trace_id);
        assert!(trace_id.is_some());
        assert_eq!(trace_id.expect("trace id").len(), 32);
        telemetry.shutdown().expect("shutdown");
    }
}
