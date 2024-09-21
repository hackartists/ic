use std::{
    borrow::Borrow,
    sync::{Arc, RwLock},
};

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    Json, Router,
};
use ic_artifact_pool::consensus_pool::{ConsensusPoolImpl, UncachedConsensusPoolImpl};
use ic_config::artifact_pool::ArtifactPoolConfig;
use ic_interfaces::consensus_pool::ConsensusBlockCache;
use ic_interfaces_state_manager::StateReader;
use ic_replicated_state::{CanisterState, ReplicatedState};
use ic_types::{
    consensus::{Block, HashedBlock},
    crypto::{crypto_hash, CryptoHash, CryptoHashOf},
    CanisterId, Height,
};
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
            axum::routing::get(get_block_at)
                .with_state(consensus_pool.clone())
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
#[serde(untagged)]
enum CallResponse {
    Block(GetBlock),
    Err(Error),
}

#[derive(Serialize)]
struct Error {
    message: String,
}

#[derive(Serialize)]
struct GetBlock {
    prev_hash: String,
    height: u64,
    block_hash: String,
}

impl From<&Block> for GetBlock {
    fn from(block: &Block) -> Self {
        let prev_hash = format!("0x{}", hex::encode(block.clone().parent.get().0));
        let height = block.clone().height.get();

        let block_hash = HashedBlock::new(crypto_hash, block.clone());
        let block_hash = format!("0x{}", hex::encode(block_hash.get_hash().clone().get().0));
        Self {
            prev_hash,
            height,
            block_hash,
        }
    }
}

async fn get_block_at(
    Path(height): Path<u64>,
    State(consensus_pool): State<Arc<RwLock<ConsensusPoolImpl>>>,
) -> Json<CallResponse> {
    let pool = consensus_pool
        .read()
        .expect("Failed to read consensus pool");

    let height = Height::new(height);
    let chain = pool.finalized_chain();
    let block = match chain.get_block_by_height(height) {
        Ok(block) => GetBlock::from(block),
        Err(_) => {
            return Json(CallResponse::Err(Error {
                message: "block not found".to_string(),
            }))
        }
    };

    Json(CallResponse::Block(block))
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
