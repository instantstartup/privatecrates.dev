use std::sync::Arc;

use privatecrates_server::{AppState, config::Config, router, spawn_refresh};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Held until the server stops: dropping it flushes the last logs and spans.
    let _telemetry = privatecrates_server::telemetry::start()?;
    let config = Config::from_env()?;
    let bind = config.bind;
    let state = Arc::new(AppState::new(config)?);
    state.terms.records().migrate().await?;
    state.discover().await?;
    log::info!(tenants = state.tenants.all().len(); "tenants discovered");
    state.billing.load().await?;
    spawn_refresh(state.clone());
    let listener = tokio::net::TcpListener::bind(bind).await?;
    log::info!(bind:% = bind; "listening");
    axum::serve(listener, router(state)).await?;
    Ok(())
}
