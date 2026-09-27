use std::sync::Arc;

use privatecrates_server::{AppState, config::Config, router, spawn_refresh};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();
    let config = Config::from_env()?;
    let bind = config.bind;
    let state = Arc::new(AppState::new(config)?);
    state.discover().await?;
    tracing::info!(tenants = state.tenants.all().len(), "tenants discovered");
    spawn_refresh(state.clone());
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "listening");
    axum::serve(listener, router(state)).await?;
    Ok(())
}
