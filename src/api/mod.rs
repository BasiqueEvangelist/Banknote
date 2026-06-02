use axum::Router;

use crate::state::ServerState;

pub mod nuget;

pub fn router() -> Router<ServerState> {
    Router::new()
        .merge(nuget::router())
}