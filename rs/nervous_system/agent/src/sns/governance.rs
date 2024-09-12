use anyhow::Result;
use ic_agent::Agent;
use ic_base_types::PrincipalId;
use ic_sns_governance::pb::v1::{
    GetMetadataRequest, GetMetadataResponse, GetRunningSnsVersionRequest,
    GetRunningSnsVersionResponse,
};
use serde::{Deserialize, Serialize};

use crate::call;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GovernanceCanister {
    pub canister_id: PrincipalId,
}

impl GovernanceCanister {
    pub async fn metadata(&self, agent: &Agent) -> Result<GetMetadataResponse> {
        call(agent, self.canister_id, GetMetadataRequest {}).await
    }

    pub async fn version(&self, agent: &Agent) -> Result<GetRunningSnsVersionResponse> {
        call(agent, self.canister_id, GetRunningSnsVersionRequest {}).await
    }
}
