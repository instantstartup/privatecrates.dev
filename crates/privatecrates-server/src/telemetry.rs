//! Logs, spans and metrics through apollo-opentelemetry. Logs are written with the `log` crate's macros, which the
//! log bridge routes to OpenTelemetry; spans come from the request layer in `lib.rs`.

use apollo_configuration::ConfigParser;
use apollo_opentelemetry::{OpenTelemetryConfig, Telemetry};

/// Logs to standard output only: local development, and any deployment without Grafana Cloud.
const DEFAULT: &str = include_str!("telemetry.yaml");
/// Logs, spans and metrics to Grafana Cloud as well, when `GRAFANA_CLOUD_API_KEY` is set.
const GRAFANA_CLOUD: &str = include_str!("telemetry-grafana-cloud.yaml");

/// Starts telemetry. Keep the handle until shutdown: dropping it flushes and stops the exporters.
///
/// The configuration is the file `TELEMETRY_CONFIG` names, if set; otherwise Grafana Cloud's when its API key is
/// set; otherwise standard output only.
pub fn start() -> Result<Telemetry, Box<dyn std::error::Error>> {
    let yaml = match std::env::var("TELEMETRY_CONFIG") {
        Ok(path) => {
            std::fs::read_to_string(&path).map_err(|e| format!("TELEMETRY_CONFIG {path}: {e}"))?
        }
        Err(_) if std::env::var_os("GRAFANA_CLOUD_API_KEY").is_some() => GRAFANA_CLOUD.to_owned(),
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
    fn parse(yaml: &str) {
        let parser = apollo_configuration::ConfigParser::<apollo_opentelemetry::OpenTelemetryConfig>::builder()
            .build()
            .unwrap();
        parser.parse_yaml(yaml).unwrap();
    }

    #[test]
    fn the_default_configuration_parses() {
        parse(super::DEFAULT);
    }

    #[test]
    fn the_grafana_cloud_configuration_parses() {
        // What Railway would provide; none of it real.
        let yaml = super::GRAFANA_CLOUD
            .replace("${env.GRAFANA_CLOUD_INSTANCE_ID}", "123456")
            .replace("${env.GRAFANA_CLOUD_API_KEY}", "glc_not_a_real_key")
            .replace(
                "${env.GRAFANA_CLOUD_OTLP_ENDPOINT}",
                "https://otlp-gateway-prod-eu-west-2.grafana.net/otlp",
            );
        parse(&yaml);
    }
}
