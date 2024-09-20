use std::sync::{Arc, RwLock};

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    Json, Router,
};
use ic_artifact_pool::consensus_pool::ConsensusPoolImpl;
use ic_interfaces_state_manager::StateReader;
use ic_replicated_state::{CanisterState, ReplicatedState};
use ic_types::{CanisterId, Height};
use serde::Serialize;
use tower::ServiceBuilder;

pub(crate) fn route() -> &'static str {
    "/api/v4"
}

pub(crate) fn new_router(
    state_reader: Arc<dyn StateReader<State = ReplicatedState>>,
    consensus_pool: Arc<RwLock<ConsensusPoolImpl>>,
) -> Router {
    Router::new()
        .route_service(
            "/api/v4/height",
            axum::routing::get(get_height)
                .with_state(state_reader.clone())
                .layer(ServiceBuilder::new().layer(DefaultBodyLimit::disable())),
        )
        .route_service(
            "/api/v4/state/:height",
            axum::routing::get(get_state_at)
                .with_state(state_reader.clone())
                .layer(ServiceBuilder::new().layer(DefaultBodyLimit::disable())),
        )
    // .route_service(
    //     "/api/v4/pool/:height",
    //     axum::routing::get(get_state_at)
    //         .with_state(consensus_pool)
    //         .layer(ServiceBuilder::new().layer(DefaultBodyLimit::disable())),
    // )
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

#[derive(Serialize)]
struct GetState {
    prev_hash: String,
    height: u64,
    canister_states: Vec<CanisterStateItem>,
}

#[derive(Serialize)]
struct CanisterStateItem {
    canister_id: String,
    state: String,
}

async fn get_state_at(
    Path(height): Path<u64>,
    State(state): State<Arc<dyn StateReader<State = ReplicatedState>>>,
) -> Json<GetState> {
    let height = Height::new(height);
    let state = state
        .get_state_at(height)
        .expect("Failed to get state at height.");
    let height = state.height().get();
    let state = state.get_ref();
    let prev_hash = match height {
        0 => "genesis".to_string(),
        _ => format!(
            "{:?}",
            state
                .system_metadata()
                .prev_state_hash
                .clone()
                .expect("Failed to get prev hash")
        ),
    };
    let canister_states = state
        .canister_states
        .iter()
        .map(|(canister_id, canister_state)| CanisterStateItem {
            canister_id: canister_id.get().to_string(),
            state: format!("{:?}", canister_state),
        })
        .collect();

    Json(GetState {
        prev_hash,
        height,
        canister_states,
    })
}

// async fn get_state_at(
//     Path(height): Path<u64>,
//     State(pool): State<Arc<RwLock<ConsensusPoolImpl>>>,
// ) -> Json<GetState> {
//     let pool = pool.read().expect("Failed to read consensus pool");
//     pool.
//     // Json(GetState {
//     //     prev_hash,
//     //     height,
//     //     canister_states,
//     // })
// }
