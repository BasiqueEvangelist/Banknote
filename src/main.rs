use std::net::SocketAddr;

use axum::Router;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;
use url::Url;

pub mod state;
pub mod api;
pub mod nuget;
pub mod error;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv()?;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let state = state::init().await?;

    let mut app = api::router()
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let base_url = Url::parse(state.base_url)?;
    if base_url.path() != "/" {
        app = Router::new()
            .nest(&base_url.path(), app);
    }

    let listener = tokio::net::TcpListener::bind(state.bind_addr)
        .await
        .unwrap();
    
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();

    Ok(())
}
