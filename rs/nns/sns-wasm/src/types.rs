use crate::pb::v1::{ListUpgradeStepsResponse, SnsVersion};

impl ListUpgradeStepsResponse {
    pub fn version_number(&self, v: SnsVersion) -> Option<usize> {
        self.steps
            .iter()
            .position(|x| x.version == Some(v.clone()))
            .map(|x| x + 1)
    }

    pub fn latest_version_number(&self) -> usize {
        self.steps.len()
    }
}
