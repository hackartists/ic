use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, State},
    Json, Router,
};
use ic_interfaces_state_manager::StateReader;
use ic_replicated_state::ReplicatedState;
use serde::Serialize;
use tower::ServiceBuilder;

pub(crate) fn route() -> &'static str {
    "/api/v4"
}

pub(crate) fn new_router(state_reader: Arc<dyn StateReader<State = ReplicatedState>>) -> Router {
    Router::new().route_service(
        "/api/v4/height",
        axum::routing::get(get_height)
            .with_state(state_reader)
            .layer(ServiceBuilder::new().layer(DefaultBodyLimit::disable())),
    )
}

#[derive(Serialize)]
struct GetHeight {
    height: u64,
}

async fn get_height(
    State(state): State<Arc<dyn StateReader<State = ReplicatedState>>>,
) -> Json<GetHeight> {
    let height = state.latest_state_height().get();

    Json(GetHeight { height })
}
