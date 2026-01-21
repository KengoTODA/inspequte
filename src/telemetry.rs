use anyhow::Result;
use opentelemetry::KeyValue;
use opentelemetry::global::{self, BoxedSpan, BoxedTracer};
use opentelemetry::trace::{Span, Tracer};
use opentelemetry_otlp::{Protocol, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;

/// OpenTelemetry integration for tracing inspequte execution.
pub(crate) struct Telemetry {
    enabled: bool,
    provider: Option<SdkTracerProvider>,
    tracer: BoxedTracer,
}

impl Telemetry {
    pub(crate) fn new() -> Result<Self> {
        let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
        if let Some(endpoint) = endpoint {
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(endpoint)
                .build()?;
            let resource = Resource::builder().with_service_name("inspequte").build();
            let provider = SdkTracerProvider::builder()
                .with_simple_exporter(exporter)
                .with_resource(resource)
                .build();
            global::set_tracer_provider(provider.clone());
            let tracer = global::tracer("inspequte");
            Ok(Self {
                enabled: true,
                provider: Some(provider),
                tracer,
            })
        } else {
            Ok(Self::disabled())
        }
    }

    pub(crate) fn disabled() -> Self {
        Self {
            enabled: false,
            provider: None,
            tracer: global::tracer("inspequte"),
        }
    }

    pub(crate) fn span(&self, name: &str, attributes: Vec<KeyValue>) -> TelemetrySpan {
        if !self.enabled {
            return TelemetrySpan::disabled();
        }
        let mut span = self.tracer.start(name.to_string());
        span.set_attributes(attributes);
        TelemetrySpan::new(span)
    }

    pub(crate) fn shutdown(&self) {
        if let Some(provider) = &self.provider {
            let _ = provider.shutdown();
        }
    }
}

/// RAII guard for OpenTelemetry spans.
pub(crate) struct TelemetrySpan {
    span: Option<BoxedSpan>,
}

impl TelemetrySpan {
    fn new(span: BoxedSpan) -> Self {
        Self { span: Some(span) }
    }

    fn disabled() -> Self {
        Self { span: None }
    }
}

impl Drop for TelemetrySpan {
    fn drop(&mut self) {
        if let Some(mut span) = self.span.take() {
            span.end();
        }
    }
}
