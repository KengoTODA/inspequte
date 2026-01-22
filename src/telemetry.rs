use anyhow::{Context, Result, anyhow};
use opentelemetry::trace::{Span, TraceContextExt, Tracer, TracerProvider as OtelTracerProvider};
use opentelemetry::{Context as OtelContext, KeyValue};
use opentelemetry_otlp::{SpanExporterBuilder, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::export::trace::SpanExporter;
use opentelemetry_sdk::runtime::Tokio;
use opentelemetry_sdk::trace::{BatchConfigBuilder, BatchSpanProcessor, Config, TracerProvider};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Telemetry handle for OpenTelemetry tracing.
pub(crate) struct Telemetry {
    tracer: opentelemetry_sdk::trace::Tracer,
    provider: TracerProvider,
    _runtime: tokio::runtime::Runtime,
}

impl Telemetry {
    /// Initialize telemetry with an OTLP HTTP exporter.
    pub(crate) fn new(endpoint: String) -> Result<Self> {
        let endpoint = normalize_otlp_http_endpoint(&endpoint)?;
        let exporter = SpanExporterBuilder::from(
            opentelemetry_otlp::new_exporter()
                .http()
                .with_endpoint(endpoint)
                .with_http_client(reqwest::Client::new()),
        )
        .build_span_exporter()
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
        let resource_attributes = vec![KeyValue::new("service.name", "inspequte")];
        let resource = Resource::new(resource_attributes);
        install_error_handler();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .context("build Tokio runtime")?;
        let _guard = runtime.enter();
        let batch_config = BatchConfigBuilder::default()
            .with_max_queue_size(65_536)
            .with_max_export_batch_size(4096)
            .with_scheduled_delay(Duration::from_millis(200))
            .with_max_export_timeout(Duration::from_secs(10))
            .with_max_concurrent_exports(2)
            .build();
        let processor = BatchSpanProcessor::builder(exporter, Tokio)
            .with_batch_config(batch_config)
            .build();
        let provider = TracerProvider::builder()
            .with_span_processor(processor)
            .with_config(Config::default().with_resource(resource))
            .build();
        let tracer = provider.tracer("inspequte");
        opentelemetry::global::set_tracer_provider(provider.clone());
        Ok(Self {
            tracer,
            provider,
            _runtime: runtime,
        })
    }
}

fn normalize_otlp_http_endpoint(endpoint: &str) -> Result<String> {
    let mut url = reqwest::Url::parse(endpoint).context("parse OTLP endpoint")?;
    if url.path() == "/" {
        url.set_path("/v1/traces");
    }
    Ok(url.to_string())
}

fn install_error_handler() {
    static SET_ERROR_HANDLER: Once = Once::new();
    static LOGGED_ERROR: AtomicBool = AtomicBool::new(false);
    SET_ERROR_HANDLER.call_once(|| {
        let _ = opentelemetry::global::set_error_handler(move |err| {
            let message = err.to_string();
            if LOGGED_ERROR.swap(true, Ordering::Relaxed) {
                return;
            }
            eprintln!("OpenTelemetry trace error occurred: {message}");
        });
    });
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
    use futures_util::future::BoxFuture;
    use opentelemetry_sdk::export::trace::{ExportResult, SpanData, SpanExporter};

    #[derive(Debug)]
    struct NoopExporter;

    impl SpanExporter for NoopExporter {
        fn export(&mut self, _batch: Vec<SpanData>) -> BoxFuture<'static, ExportResult> {
            Box::pin(async { Ok(()) })
        }
    }

    #[test]
    fn telemetry_uses_exporter_without_errors() {
        let telemetry = Telemetry::from_exporter(NoopExporter).expect("telemetry");
        telemetry.in_span("test", &[KeyValue::new("test.key", "value")], || {});
        telemetry.shutdown().expect("shutdown");
    }
}
