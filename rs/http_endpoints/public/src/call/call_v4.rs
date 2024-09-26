use std::{
    sync::{Arc, RwLock},
    thread::sleep,
    time::Duration,
};

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    Json, Router,
};
use ic_artifact_pool::consensus_pool::ConsensusPoolImpl;
use ic_interfaces::consensus_pool::HeightRange;
use ic_interfaces_state_manager::StateReader;
use ic_replicated_state::ReplicatedState;
use ic_types::{
    consensus::{Block, HasHeight, HashedBlock},
    crypto::crypto_hash,
    CanisterId, Height,
};
use serde::{Deserialize, Serialize};
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
            "/api/v4/block/:height",
            axum::routing::get(get_block_at)
                .with_state(consensus_pool.clone())
                .layer(ServiceBuilder::new().layer(DefaultBodyLimit::disable())),
        )
        .route_service(
            "/api/v4/blocks",
            axum::routing::get(list_blocks)
                .with_state(consensus_pool.clone())
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

#[derive(Serialize)]
#[serde(untagged)]
enum CallResponse {
    Block(GetBlock),
    Blocks(Vec<GetBlock>),
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
    ingress_messages: Option<Vec<IngressMessage>>,
}

#[derive(Serialize)]
struct IngressMessage {
    message_id: String,
    canister_id: CanisterId,
    method_name: String,
    sender: String,
}

impl GetBlock {
    pub fn set_ingress_messages(&mut self, messages: Vec<IngressMessage>) {
        self.ingress_messages = Some(messages);
    }
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
            ingress_messages: None,
        }
    }
}

#[derive(Deserialize)]
struct BlockRange {
    from: u64,
    to: u64,
}

async fn list_blocks(
    range: Query<BlockRange>,
    State(consensus_pool): State<Arc<RwLock<ConsensusPoolImpl>>>,
) -> Json<CallResponse> {
    let pool = consensus_pool
        .read()
        .expect("Failed to read consensus pool");

    let finalizations = pool
        .validated
        .finalization()
        .get_by_height_range(HeightRange {
            min: Height::new(range.from),
            max: Height::new(range.to),
        });
    let mut blocks = vec![];

    for finalization in finalizations {
        let block_hash = &finalization.content.block;
        let mut block = None;
        let mut ingress_messages = vec![];

        for proposal in pool
            .validated
            .block_proposal()
            .get_by_height(finalization.height())
        {
            let blk: Block = proposal.content.clone().into_inner();

            if !blk.payload.is_summary() {
                let batch = &blk.payload.as_ref().as_data().batch;
                let count = batch.ingress.message_count();

                for i in 0..count {
                    let (message_id, message) = batch.ingress.get(i).unwrap();
                    let tx_id = format!("0x{}", message_id.message_id);
                    let tx_content = message.as_ref().content();
                    let canister_id = tx_content.canister_id();
                    let method_name = tx_content.method_name();
                    let sender = tx_content.sender().get().0.to_text();

                    ingress_messages.push(IngressMessage {
                        message_id: tx_id,
                        canister_id,
                        method_name: method_name.to_string(),
                        sender,
                    });
                }
            };

            if proposal.content.get_hash() == block_hash {
                block = Some(proposal.content.clone().into_inner());
            }
        }

        if block.is_none() {
            return Json(CallResponse::Err(Error {
                message: format!(
                    "{} block not found: {:?}",
                    finalization.height().get(),
                    block_hash
                ),
            }));
        }
        let block = block.unwrap();

        let mut block = GetBlock::from(&block);
        if ingress_messages.len() > 0 {
            block.set_ingress_messages(ingress_messages);
        }

        blocks.push(block);
    }

    Json(CallResponse::Blocks(blocks))
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
        Ok(block) => {
            let messages = if block.payload.is_summary() {
                None
            } else {
                let batch = &block.payload.as_ref().as_data().batch;
                let mut ingress_messages = vec![];
                let count = batch.ingress.message_count();

                for i in 0..count {
                    let (message_id, message) = batch.ingress.get(i).unwrap();
                    let tx_id = format!("0x{}", message_id.message_id);
                    let tx_content = message.as_ref().content();
                    let canister_id = tx_content.canister_id();
                    let method_name = tx_content.method_name();
                    let sender = tx_content.sender().get().0.to_text();

                    ingress_messages.push(IngressMessage {
                        message_id: tx_id,
                        canister_id,
                        method_name: method_name.to_string(),
                        sender,
                    });
                }

                Some(ingress_messages)
            };

            let mut block = GetBlock::from(block);
            if let Some(messages) = messages {
                block.set_ingress_messages(messages);
            }

            block
        }
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
