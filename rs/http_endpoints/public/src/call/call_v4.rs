use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    Json, Router,
};
use bincode::de::read;
use http_body_util::Limited;
use ic_artifact_pool::consensus_pool::ConsensusPoolImpl;
use ic_config::artifact_pool::{ArtifactPoolConfig, LMDBConfig, PersistentPoolBackend};
use ic_interfaces::consensus_pool::HeightRange;
use ic_interfaces_state_manager::StateReader;
use ic_replicated_state::ReplicatedState;
use ic_types::{
    consensus::{Block, BlockProposal, HasHeight, HashedBlock},
    crypto::crypto_hash,
    CanisterId, Height,
};
use lmdb::Transaction;
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;

pub(crate) fn route() -> &'static str {
    "/api/v4"
}

pub(crate) fn new_router(
    state_reader: Arc<dyn StateReader<State = ReplicatedState>>,
    consensus_pool: Arc<RwLock<ConsensusPoolImpl>>,
    artifact_pool_config: PersistentPoolBackend,
) -> Router {
    let artifact_pool_config = match artifact_pool_config {
        PersistentPoolBackend::Lmdb(lmdb_config) => Arc::new(lmdb_config),
        _ => panic!("Unsupported persistent pool backend"),
    };

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
                .with_state((consensus_pool.clone(), artifact_pool_config.clone()))
                .layer(ServiceBuilder::new().layer(DefaultBodyLimit::disable())),
        )
    // .route_service(
    //     "/api/v4/blocks",
    //     axum::routing::get(list_blocks)
    //         .with_state(consensus_pool.clone())
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

fn list_blocks(
    range: Query<BlockRange>,
    State((consensus_pool, lmdb_config)): State<(Arc<RwLock<ConsensusPoolImpl>>, Arc<LMDBConfig>)>,
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

    let log = ic_logger::no_op_logger();
    let conf = LMDBConfig {
        persistent_pool_validated_persistent_db_path: lmdb_config
            .persistent_pool_validated_persistent_db_path
            .clone(),
    };

    let pool2 = ic_artifact_pool::lmdb_pool::PersistentHeightIndexedPool::new_consensus_pool(
        conf, true, log,
    );

    for finalization in finalizations {
        let block_hash = &finalization.content.block;

        let key = ic_artifact_pool::lmdb_pool::IdKey::new(
            Height::new(1),
            ic_artifact_pool::lmdb_pool::TypeKey::BlockProposal,
            &block_hash.clone().get(),
        );
        let mut ingress_messages = vec![];
        let tx = pool2.db_env.begin_ro_txn();
        if tx.is_err() {
            continue;
        }
        let tx = tx.unwrap();

        let bytes = tx.get(pool2.artifacts, &key);
        if bytes.is_err() {
            continue;
        }

        let block_proposal = bincode::deserialize::<BlockProposal>(bytes.unwrap());
        if block_proposal.is_err() {
            continue;
        }
        let block_proposal = block_proposal.unwrap();
        let blk: Block = block_proposal.content.clone().into_inner();

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

        let mut block = GetBlock::from(&blk);
        if ingress_messages.len() > 0 {
            block.set_ingress_messages(ingress_messages);
        }

        blocks.push(block);
    }

    Json(CallResponse::Blocks(blocks))
}

async fn get_block_at(
    Path(height): Path<u64>,
    State((consensus_pool, lmdb_config)): State<(Arc<RwLock<ConsensusPoolImpl>>, Arc<LMDBConfig>)>,
) -> Json<CallResponse> {
    let pool = consensus_pool
        .read()
        .expect("Failed to read consensus pool");

    let height = Height::new(height);
    let finalization = pool.validated.finalization().get_only_by_height(height);
    if finalization.is_err() {
        return Json(CallResponse::Err(Error {
            message: "Block not found".to_string(),
        }));
    }

    let block_hash = &finalization.unwrap().content.block;
    let log = ic_logger::no_op_logger();
    let conf = LMDBConfig {
        persistent_pool_validated_persistent_db_path: lmdb_config
            .persistent_pool_validated_persistent_db_path
            .clone(),
    };

    let pool2 = ic_artifact_pool::lmdb_pool::PersistentHeightIndexedPool::new_consensus_pool(
        conf, true, log,
    );

    let key = ic_artifact_pool::lmdb_pool::IdKey::new(
        Height::new(1),
        ic_artifact_pool::lmdb_pool::TypeKey::BlockProposal,
        &block_hash.clone().get(),
    );
    let mut ingress_messages = vec![];
    let tx = pool2.db_env.begin_ro_txn();
    if tx.is_err() {
        return Json(CallResponse::Err(Error {
            message: "Block not found".to_string(),
        }));
    }
    let tx = tx.unwrap();

    let bytes = tx.get(pool2.artifacts, &key);
    if bytes.is_err() {
        return Json(CallResponse::Err(Error {
            message: "Block not found".to_string(),
        }));
    }

    let block_proposal = bincode::deserialize::<BlockProposal>(bytes.unwrap());
    if block_proposal.is_err() {
        return Json(CallResponse::Err(Error {
            message: "Block not found".to_string(),
        }));
    }
    let block_proposal = block_proposal.unwrap();
    let blk: Block = block_proposal.content.clone().into_inner();

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

    let mut block = GetBlock::from(&blk);
    if ingress_messages.len() > 0 {
        block.set_ingress_messages(ingress_messages);
    }

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
