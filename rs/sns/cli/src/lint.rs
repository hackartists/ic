use crate::table::TableRow;
use anyhow::{anyhow, Result};
use clap::Parser;
use futures::{stream, StreamExt};
use ic_agent::Agent;
use ic_nervous_system_agent::nns::sns_wasm;
use itertools::Itertools;

/// The arguments used to configure the lint command
#[derive(Debug, Parser)]
pub struct LintArgs {}

struct SnsLintInfo {
    name: String,
    high_memory_consumption: Vec<(String, u64)>,
    low_cycles: Vec<(String, u64)>,
    num_remaining_upgrade_steps: usize,
}

impl TableRow for SnsLintInfo {
    fn column_names() -> Vec<&'static str> {
        vec!["Name", "Upgrades Remaining"]
    }

    fn column_values(&self) -> Vec<String> {
        let memory_consumption = self
            .high_memory_consumption
            .iter()
            .map(|(canister_type, memory_consumption)| {
                format!(
                    "{canister_type} ({:.2} GiB)",
                    *memory_consumption as f64 / 1024.0 / 1024.0 / 1024.0
                )
            })
            .join(", ");
        let memory_consumption = if !memory_consumption.is_empty() {
            format!("❌ {memory_consumption}")
        } else {
            "👍".to_string()
        };
        let low_cycles = self
            .low_cycles
            .iter()
            .map(|(canister_type, cycles)| {
                format!(
                    "{canister_type} ({:.2} TC)",
                    *cycles as f64 / 1000.0 / 1000.0 / 1000.0 / 1000.0
                )
            })
            .join(", ");
        let low_cycles = if !low_cycles.is_empty() {
            format!("❌ {low_cycles}")
        } else {
            "👍".to_string()
        };
        vec![
            self.name.clone(),
            format!("{}", self.num_remaining_upgrade_steps),
        ]
    }
}

pub async fn exec(_args: LintArgs, agent: &Agent) -> Result<()> {
    println!("Checking SNSes...");

    let snses = sns_wasm::list_deployed_snses(agent).await?;
    let num_total_snses = snses.len();
    let snses_with_metadata = stream::iter(snses)
        .map(|sns| async move {
            let metadata = sns.governance.metadata(agent).await?;
            Ok((sns, metadata))
        })
        .buffer_unordered(10) // Do up to 10 requests at a time in parallel
        .collect::<Vec<anyhow::Result<_>>>()
        .await;
    let snses_with_metadata = snses_with_metadata
        .into_iter()
        .filter_map(Result::ok)
        .collect::<Vec<_>>();

    let num_snses_with_metadata = snses_with_metadata.len();

    let lint_info: Vec<SnsLintInfo> = stream::iter(snses_with_metadata)
        .map(|(sns, metadata)| async move {
            let summary = sns.root.sns_canisters_summary(agent).await?;
            let name = metadata.name.ok_or(anyhow!("SNS has no name"))?;

            let governance_summary = summary.governance.ok_or(anyhow!(
                "SNS {name} canister summary is missing `governance`"
            ))?;
            let governance_status = governance_summary
                .status
                .ok_or(anyhow!("SNS {name} `governance` has no status"))?;

            let root_summary = summary
                .root
                .ok_or(anyhow!("SNS {name} canister summary is missing `root`"))?;
            let root_status = root_summary
                .status
                .ok_or(anyhow!("SNS {name} `root` has no status"))?;

            let swap_summary = summary
                .swap
                .ok_or(anyhow!("SNS {name} canister summary is missing `swap`"))?;
            let swap_status = swap_summary
                .status
                .ok_or(anyhow!("SNS {name} `swap` has no status"))?;

            let high_memory_consumption = {
                let governance_memory_consumption =
                    { u64::try_from(governance_status.memory_size.0).unwrap() };

                let root_memory_consumption = { u64::try_from(root_status.memory_size.0).unwrap() };

                let swap_memory_consumption = { u64::try_from(swap_status.memory_size.0).unwrap() };

                [
                    ("governance", governance_memory_consumption),
                    ("root", root_memory_consumption),
                    ("swap", swap_memory_consumption),
                ]
                .iter()
                .filter(|(_, memory_consumption)| {
                    (*memory_consumption as f64) > 2.5 * 1024.0 * 1024.0 * 1024.0
                })
                .map(|(canister, memory_consumption)| (canister.to_string(), *memory_consumption))
                .collect::<Vec<_>>()
            };

            let low_cycles = {
                let governance_cycles = { u64::try_from(governance_status.cycles.0).unwrap() };

                let root_cycles = { u64::try_from(root_status.cycles.0).unwrap() };

                let swap_cycles = { u64::try_from(swap_status.cycles.0).unwrap() };

                [
                    ("governance", governance_cycles),
                    ("root", root_cycles),
                    ("swap", swap_cycles),
                ]
                .iter()
                .filter(|(_, cycles)| (*cycles as f64) < 10.0 * 1000.0 * 1000.0 * 1000.0 * 1000.0)
                .map(|(canister, cycles)| (canister.to_string(), *cycles))
                .collect::<Vec<_>>()
            };

            let num_remaining_upgrade_steps =
                sns.remaining_upgrade_steps(agent).await?.steps.len() - 1;

            Result::<SnsLintInfo, anyhow::Error>::Ok(SnsLintInfo {
                name,
                high_memory_consumption,
                low_cycles,
                num_remaining_upgrade_steps,
            })
        })
        .buffer_unordered(10)
        .collect::<Vec<Result<_>>>()
        .await
        .into_iter()
        .inspect(|result| match result {
            Err(e) => println!("Error: {}", e),
            _ => {}
        })
        .filter_map(Result::ok)
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .collect::<Vec<_>>();

    let lint_info_table = crate::table::as_table(lint_info.as_ref());
    println!("{}", lint_info_table);
    eprintln!(
        "Out of {num_total_snses} SNSes, {num_snses_with_metadata} had metadata and I linted {num_linted} of them.",
        num_linted = lint_info.len()
    );

    Ok(())
}
