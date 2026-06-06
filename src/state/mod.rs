use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use anyhow::Context;
use axum::{extract::FromRequestParts, http::request::Parts};

pub mod config;
pub mod nuget;

#[derive(Clone)]
pub struct ServerState {
    pub bind_addr: SocketAddr,
    pub base_url: &'static str,

    pub nuget: Option<Arc<nuget::NugetState>>,
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
    let config: config::BanknoteConfig = knus::parse(
        "config.kdl",
        &std::fs::read_to_string("./config.kdl").with_context(|| "reading config file")?,
    )?;

    let cache_path: &'static str = config.cache.path.leak();

    std::fs::create_dir_all(cache_path)
        .with_context(|| format!("creating cache directory {}", cache_path))?;

    let mut nuget: Option<Arc<nuget::NugetState>> = None;

    if let Some(nuget_config) = &config.repositories.nuget {
        nuget = Some(Arc::new(
            nuget::init(cache_path, nuget_config)
                .await
                .with_context(|| "initializing NuGet mirror")?,
        ));
    }

    Ok(ServerState {
        bind_addr: config.bind_addr.parse()?,
        base_url: config.base_url.leak(),
        nuget,
    })
}
