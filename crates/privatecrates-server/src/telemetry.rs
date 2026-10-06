//! Logs, spans and metrics through apollo-opentelemetry. Logs are written with the `log` crate's macros, which the
//! log bridge routes to OpenTelemetry; spans come from the request layer in `lib.rs`.

use apollo_configuration::ConfigParser;
use apollo_opentelemetry::{OpenTelemetryConfig, Telemetry};

/// The default configuration, used unless `TELEMETRY_CONFIG` names a file.
const DEFAULT: &str = include_str!("telemetry.yaml");

/// Starts telemetry. Keep the handle until shutdown: dropping it flushes and stops the exporters.
pub fn start() -> Result<Telemetry, Box<dyn std::error::Error>> {
    let yaml = match std::env::var("TELEMETRY_CONFIG") {
        Ok(path) => {
            std::fs::read_to_string(&path).map_err(|e| format!("TELEMETRY_CONFIG {path}: {e}"))?
        }
        Err(_) => DEFAULT.to_owned(),
    };
    let config: OpenTelemetryConfig = ConfigParser::builder().build()?.parse_yaml(&yaml)?;
    Ok(Telemetry::builder(config)
        .with_global_tracer_provider()
        .with_global_meter_provider()
        .with_log_bridge()
        .build()?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_default_configuration_parses() {
        let parser = apollo_configuration::ConfigParser::<apollo_opentelemetry::OpenTelemetryConfig>::builder()
            .build()
            .unwrap();
        parser.parse_yaml(super::DEFAULT).unwrap();
    }
}
