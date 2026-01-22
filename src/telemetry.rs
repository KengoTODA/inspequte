use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use futures_util::future::BoxFuture;
use opentelemetry::trace::{
    SpanId, SpanKind, TraceContextExt, TraceId, Tracer, TracerProvider as OtelTracerProvider,
};
use opentelemetry::{KeyValue, Value};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::export::trace::{ExportResult, SpanData, SpanExporter};
use opentelemetry_sdk::trace::{Config, SimpleSpanProcessor, TracerProvider};
use serde::Serialize;

/// Telemetry handle for OpenTelemetry tracing.
pub(crate) struct Telemetry {
    tracer: opentelemetry_sdk::trace::Tracer,
    span_store: Arc<Mutex<Vec<SpanData>>>,
    file_path: PathBuf,
    provider: TracerProvider,
    resource_attributes: Vec<KeyValue>,
}

impl Telemetry {
    /// Initialize telemetry with a file exporter.
    pub(crate) fn new(file_path: PathBuf) -> Result<Self> {
        let span_store = Arc::new(Mutex::new(Vec::new()));
        let exporter = SpanStoreExporter {
            spans: span_store.clone(),
        };
        let resource_attributes = vec![KeyValue::new("service.name", "inspequte")];
        let resource = Resource::new(resource_attributes.clone());
        let provider = TracerProvider::builder()
            .with_span_processor(SimpleSpanProcessor::new(Box::new(exporter)))
            .with_config(Config::default().with_resource(resource))
            .build();
        let tracer = provider.tracer("inspequte");
        opentelemetry::global::set_tracer_provider(provider.clone());
        Ok(Self {
            tracer,
            span_store,
            file_path,
            provider,
            resource_attributes,
        })
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

    /// Flush spans and write OTLP JSON to the configured file.
    pub(crate) fn shutdown(&self) -> Result<()> {
        if let Err(err) = self.provider.shutdown() {
            return Err(anyhow!("failed to shutdown tracer provider: {err}"));
        }
        let spans = {
            let mut guard = self.span_store.lock().expect("span store lock");
            std::mem::take(&mut *guard)
        };
        let export = export_trace_request(&self.resource_attributes, &spans)?;
        let file = File::create(&self.file_path)
            .with_context(|| format!("failed to open {}", self.file_path.display()))?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, &export).context("failed to serialize OTLP JSON")?;
        writer
            .write_all(b"\n")
            .context("failed to write OTLP JSON")?;
        Ok(())
    }
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

/// In-memory span exporter that buffers spans for later JSON file output.
#[derive(Debug)]
struct SpanStoreExporter {
    spans: Arc<Mutex<Vec<SpanData>>>,
}

impl SpanExporter for SpanStoreExporter {
    fn export(&mut self, batch: Vec<SpanData>) -> BoxFuture<'static, ExportResult> {
        let spans = self.spans.clone();
        Box::pin(async move {
            spans.lock().expect("span store lock").extend(batch);
            Ok(())
        })
    }
}

/// OTLP/JSON payload aligned with the OpenTelemetry file exporter spec:
/// https://opentelemetry.io/docs/specs/otel/protocol/file-exporter/
/// Root OTLP/JSON request containing all resource spans.
#[derive(Serialize)]
struct ExportTraceServiceRequest {
    #[serde(rename = "resourceSpans")]
    resource_spans: Vec<ResourceSpans>,
}

/// Resource-scoped spans payload.
#[derive(Serialize)]
struct ResourceSpans {
    resource: ResourceData,
    #[serde(rename = "scopeSpans")]
    scope_spans: Vec<ScopeSpans>,
}

/// Resource metadata for OTLP export.
#[derive(Serialize)]
struct ResourceData {
    attributes: Vec<KeyValueData>,
}

/// Spans grouped under a single instrumentation scope.
#[derive(Serialize)]
struct ScopeSpans {
    scope: ScopeData,
    spans: Vec<SpanDataJson>,
}

/// Instrumentation scope identity for exported spans.
#[derive(Serialize)]
struct ScopeData {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

/// OTLP/JSON span entry.
#[derive(Serialize)]
struct SpanDataJson {
    #[serde(rename = "traceId")]
    trace_id: String,
    #[serde(rename = "spanId")]
    span_id: String,
    #[serde(rename = "parentSpanId", skip_serializing_if = "Option::is_none")]
    parent_span_id: Option<String>,
    name: String,
    kind: String,
    #[serde(rename = "startTimeUnixNano")]
    start_time_unix_nano: String,
    #[serde(rename = "endTimeUnixNano")]
    end_time_unix_nano: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attributes: Vec<KeyValueData>,
}

/// Key/value attribute for OTLP/JSON output.
#[derive(Serialize)]
struct KeyValueData {
    key: String,
    value: AnyValue,
}

/// OTLP/JSON attribute value.
#[derive(Serialize)]
struct AnyValue {
    #[serde(rename = "stringValue", skip_serializing_if = "Option::is_none")]
    string_value: Option<String>,
    #[serde(rename = "boolValue", skip_serializing_if = "Option::is_none")]
    bool_value: Option<bool>,
    #[serde(rename = "intValue", skip_serializing_if = "Option::is_none")]
    int_value: Option<String>,
    #[serde(rename = "doubleValue", skip_serializing_if = "Option::is_none")]
    double_value: Option<f64>,
}

fn export_trace_request(
    resource_attributes: &[KeyValue],
    spans: &[SpanData],
) -> Result<ExportTraceServiceRequest> {
    let mut scope_map: BTreeMap<String, Vec<&SpanData>> = BTreeMap::new();
    for span in spans {
        let scope = &span.instrumentation_lib;
        let key = format!(
            "{}:{}",
            scope.name.as_ref(),
            scope.version.as_deref().unwrap_or("")
        );
        scope_map.entry(key).or_default().push(span);
    }

    let mut scope_spans = Vec::new();
    for spans in scope_map.values() {
        let scope = &spans[0].instrumentation_lib;
        let scope_data = ScopeData {
            name: scope.name.as_ref().to_string(),
            version: scope.version.as_deref().map(str::to_string),
        };
        let mut span_entries = Vec::new();
        for span in spans.iter() {
            span_entries.push(span_to_json(span)?);
        }
        scope_spans.push(ScopeSpans {
            scope: scope_data,
            spans: span_entries,
        });
    }

    Ok(ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: ResourceData {
                attributes: resource_attributes.iter().map(key_value_to_json).collect(),
            },
            scope_spans,
        }],
    })
}

fn span_to_json(span: &SpanData) -> Result<SpanDataJson> {
    let trace_id = encode_trace_id(span.span_context.trace_id());
    let span_id = encode_span_id(span.span_context.span_id());
    let parent_span_id = if span.parent_span_id == SpanId::INVALID {
        None
    } else {
        Some(encode_span_id(span.parent_span_id))
    };
    let start_time_unix_nano = system_time_to_nanos(span.start_time)?;
    let end_time_unix_nano = system_time_to_nanos(span.end_time)?;
    let attributes = span.attributes.iter().map(key_value_to_json).collect();

    Ok(SpanDataJson {
        trace_id,
        span_id,
        parent_span_id,
        name: span.name.to_string(),
        kind: span_kind_to_string(&span.span_kind),
        start_time_unix_nano,
        end_time_unix_nano,
        attributes,
    })
}

fn key_value_to_json(value: &KeyValue) -> KeyValueData {
    KeyValueData {
        key: value.key.as_str().to_string(),
        value: any_value(value.value.clone()),
    }
}

fn any_value(value: Value) -> AnyValue {
    match value {
        Value::String(value) => AnyValue {
            string_value: Some(value.to_string()),
            bool_value: None,
            int_value: None,
            double_value: None,
        },
        Value::Bool(value) => AnyValue {
            string_value: None,
            bool_value: Some(value),
            int_value: None,
            double_value: None,
        },
        Value::I64(value) => AnyValue {
            string_value: None,
            bool_value: None,
            int_value: Some(value.to_string()),
            double_value: None,
        },
        Value::F64(value) => AnyValue {
            string_value: None,
            bool_value: None,
            int_value: None,
            double_value: Some(value),
        },
        Value::Array(value) => AnyValue {
            string_value: Some(format!("{value:?}")),
            bool_value: None,
            int_value: None,
            double_value: None,
        },
    }
}

fn encode_trace_id(trace_id: TraceId) -> String {
    encode_hex(trace_id.to_bytes().as_slice())
}

fn encode_span_id(span_id: SpanId) -> String {
    encode_hex(span_id.to_bytes().as_slice())
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{:02x}", byte);
    }
    out
}

fn system_time_to_nanos(time: SystemTime) -> Result<String> {
    let nanos = time
        .duration_since(UNIX_EPOCH)
        .context("span time before unix epoch")?
        .as_nanos();
    Ok(nanos.to_string())
}

fn span_kind_to_string(kind: &SpanKind) -> String {
    match kind {
        SpanKind::Internal => "SPAN_KIND_INTERNAL",
        SpanKind::Server => "SPAN_KIND_SERVER",
        SpanKind::Client => "SPAN_KIND_CLIENT",
        SpanKind::Producer => "SPAN_KIND_PRODUCER",
        SpanKind::Consumer => "SPAN_KIND_CONSUMER",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_writes_otlp_json() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        let path = file.path().to_path_buf();
        let telemetry = Telemetry::new(path.clone()).expect("telemetry");
        telemetry.in_span("test", &[KeyValue::new("test.key", "value")], || {});
        telemetry.shutdown().expect("shutdown");
        let contents = std::fs::read_to_string(path).expect("read");
        let value: serde_json::Value = serde_json::from_str(&contents).expect("json");
        assert!(value.get("resourceSpans").is_some());
    }
}
