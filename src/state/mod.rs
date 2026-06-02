use std::{convert::Infallible, env, net::SocketAddr, sync::Arc};

use anyhow::Context;
use axum::{extract::FromRequestParts, http::request::Parts};

pub mod nuget;

#[derive(Clone)]
pub struct ServerState {
    pub bind_addr: SocketAddr,
    pub base_url: &'static str,

    pub nuget: Option<Arc<nuget::NugetState>>
}

impl FromRequestParts<ServerState> for ServerState {
    type Rejection = Infallible;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &ServerState,
    ) -> Result<Self, Self::Rejection> {
        Ok(state.clone())
    }
}

#[tracing::instrument]
pub async fn init() -> anyhow::Result<ServerState> {
    let bind_addr = env::var("BIND_ADDR")
        .with_context(|| "reading BIND_ADDR variable")?
        .parse::<SocketAddr>()
        .with_context(|| "parsing BIND_ADDR variable")?;

    let base_url = env::var("BASE_URL")
        .with_context(|| "reading BASE_URL variable")?
        .leak();

    let data_path = env::var("DATA_PATH")
        .with_context(|| "reading DATA_PATH variable")?
        .leak();

    let nuget = nuget::maybe_init(data_path).await
        .with_context(|| "initializing NuGet backend")?
        .map(Arc::new);

    Ok(ServerState {
        bind_addr,
        base_url,
        nuget
    })
}