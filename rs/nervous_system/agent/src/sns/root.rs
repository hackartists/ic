use anyhow::Result;
use ic_agent::Agent;
use ic_base_types::PrincipalId;
use serde::{Deserialize, Serialize};
use ic_sns_root::{GetSnsCanistersSummaryRequest, GetSnsCanistersSummaryResponse};

use crate::call;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RootCanister {
    pub canister_id: PrincipalId,
}

impl RootCanister {
    pub async fn sns_canisters_summary(
        &self,
        agent: &Agent,
    ) -> Result<GetSnsCanistersSummaryResponse> {
        call(
            agent,
            self.canister_id,
            GetSnsCanistersSummaryRequest {
                update_canister_list: None,
            },
        )
        .await
    }
}
