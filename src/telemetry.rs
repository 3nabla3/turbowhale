use opentelemetry::trace::TracerProvider as TracerProviderTrait;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub struct OtelGuard {
    tracer_provider: SdkTracerProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(error) = self.tracer_provider.shutdown() {
            eprintln!("Failed to shut down tracer provider: {:?}", error);
        }
    }
}

/// Initializes the OpenTelemetry tracing stack.
///
/// Reads `OTEL_BACKEND_URL` from the environment (loaded from `.env` by the caller).
/// Defaults to `http://localhost:4317` if not set.
///
/// Returns an `OtelGuard` that flushes spans when dropped.
pub fn init() -> OtelGuard {
    let backend_url = std::env::var("OTEL_BACKEND_URL")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&backend_url)
        .build()
        .expect("Failed to build OTLP span exporter");

    let tracer_provider = SdkTracerProvider::builder()
        .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(
            Resource::builder()
                .with_service_name(env!("CARGO_PKG_NAME"))
                .build(),
        )
        .with_batch_exporter(exporter)
        .build();

    let tracer = TracerProviderTrait::tracer(&tracer_provider, env!("CARGO_PKG_NAME"));

    let _ = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(OpenTelemetryLayer::new(tracer))
        .try_init();

    OtelGuard { tracer_provider }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn telemetry_init_does_not_panic() {
        // Guard is dropped at end of scope, triggering shutdown.
        let _guard = super::init();
    }
}
