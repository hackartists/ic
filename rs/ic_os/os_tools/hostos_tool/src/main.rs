use std::path::Path;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};

use config::config_ini::get_config_ini_settings;
use config::deployment_json::read_deployment_file;
use config::firewall_json;
use config::types::firewall;
use config::{
    DEFAULT_HOSTOS_CONFIG_FILE_PATH, DEFAULT_HOSTOS_DEPLOYMENT_JSON_PATH,
    DEFAULT_HOSTOS_FIREWALL_JSON_PATH,
};
use network::generate_network_config;
use network::ipv6::generate_ipv6_address;
use network::mac_address::{generate_mac_address, FormattedMacAddress};
use network::node_type::NodeType;
use network::systemd::DEFAULT_SYSTEMD_NETWORK_DIR;
use utils::to_cidr;

#[derive(Subcommand)]
pub enum Commands {
    /// Generate systemd network configuration files. Bridges available NIC's for IC IPv6 connectivity.
    GenerateNetworkConfig {
        #[arg(short, long, default_value_t = DEFAULT_SYSTEMD_NETWORK_DIR.to_string(), value_name = "DIR")]
        /// systemd-networkd output directory
        output_directory: String,
    },
    GenerateMacAddress {
        #[arg(short, long, default_value = "HostOS")]
        node_type: String,
    },
    GenerateIpv6Address {
        #[arg(short, long, default_value = "HostOS")]
        node_type: String,
    },
    RenderFirewallConfig {
        #[arg(index = 1)]
        /// Path to firewall.json.  Defaults to DEFAULT_HOSTOS_FIREWALL_JSON_PATH if unspecified.
        /// If the option is not specified, and the default file does not exist, it renders an
        /// empty firewall ruleset.  If the option is specified, and the file does not exist,
        /// it will raise an error.  If the file exists but the rules cannot be read, it will
        /// raise an error.
        firewall_file: Option<String>,
    },
}

#[derive(Parser)]
struct HostOSArgs {
    #[arg(short, long, default_value_t = DEFAULT_HOSTOS_CONFIG_FILE_PATH.to_string(), value_name = "FILE")]
    config: String,

    #[arg(short, long, default_value_t = DEFAULT_HOSTOS_DEPLOYMENT_JSON_PATH.to_string(), value_name = "FILE")]
    /// deployment.json file path
    deployment_file: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

pub fn main() -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("ERROR: this only runs on Linux.");
        std::process::exit(1);
    }

    let opts = HostOSArgs::parse();

    match opts.command {
        Some(Commands::GenerateNetworkConfig { output_directory }) => {
            let config_ini_settings = get_config_ini_settings(Path::new(&opts.config))?;
            let mut network_settings = config_ini_settings.network_settings;

            let deployment_json = read_deployment_file(Path::new(&opts.deployment_file))?;
            eprintln!("Deployment config: {:?}", deployment_json);

            // TODO: NODE-1466: Remove in configuration revamp (HostOS and GuestOS integration).
            // Once HostOS is using the config struct, all config will be contained there
            // and we won't need to read mgmt_mac from deployment.json directly.
            network_settings.mgmt_mac = deployment_json.deployment.mgmt_mac;

            eprintln!("Network settings config: {:?}", &network_settings);

            generate_network_config(
                &network_settings,
                deployment_json.deployment.name.as_str(),
                NodeType::HostOS,
                Path::new(&output_directory),
            )
        }
        Some(Commands::GenerateIpv6Address { node_type }) => {
            let config_ini_settings = get_config_ini_settings(Path::new(&opts.config))?;
            let mut network_settings = config_ini_settings.network_settings;

            let deployment_json = read_deployment_file(Path::new(&opts.deployment_file))?;
            eprintln!("Deployment config: {:?}", deployment_json);

            // TODO: NODE-1466: Remove in configuration revamp (HostOS and GuestOS integration).
            // Once HostOS is using the config struct, all config will be contained there
            // and we won't need to read mgmt_mac from deployment.json directly.
            network_settings.mgmt_mac = deployment_json.deployment.mgmt_mac.clone();

            eprintln!("Network settings config: {:?}", &network_settings);

            let node_type = node_type.parse::<NodeType>()?;
            let mac = generate_mac_address(
                &deployment_json.deployment.name,
                &node_type,
                deployment_json.deployment.mgmt_mac.as_deref(),
            )?;
            let ipv6_prefix = network_settings
                .ipv6_prefix
                .context("ipv6_prefix required in config to generate ipv6 address")?;
            let ipv6_address = generate_ipv6_address(&ipv6_prefix, &mac)?;
            println!(
                "{}",
                to_cidr(ipv6_address, network_settings.ipv6_prefix_length)
            );
            Ok(())
        }
        Some(Commands::GenerateMacAddress { node_type }) => {
            let config_ini_settings = get_config_ini_settings(Path::new(&opts.config))?;
            let mut network_settings = config_ini_settings.network_settings;

            let deployment_json = read_deployment_file(Path::new(&opts.deployment_file))?;
            eprintln!("Deployment config: {:?}", deployment_json);

            // TODO: NODE-1466: Remove in configuration revamp (HostOS and GuestOS integration).
            // Once HostOS is using the config struct, all config will be contained there
            // and we won't need to read mgmt_mac from deployment.json directly.
            network_settings.mgmt_mac = deployment_json.deployment.mgmt_mac.clone();

            eprintln!("Network settings config: {:?}", &network_settings);

            let node_type = node_type.parse::<NodeType>()?;
            let mac = generate_mac_address(
                &deployment_json.deployment.name,
                &node_type,
                deployment_json.deployment.mgmt_mac.as_deref(),
            )?;
            let mac = FormattedMacAddress::from(&mac);
            println!("{}", mac.get());
            Ok(())
        }
        Some(Commands::RenderFirewallConfig { firewall_file }) => {
            let config = firewall_json::get_firewall_rules_json_or_default(
                firewall_file.as_ref().map(Path::new),
                Path::new(DEFAULT_HOSTOS_FIREWALL_JSON_PATH),
            )?;
            eprintln!(
                "Firewall config ({}): {:#?}",
                match firewall_file {
                    Some(f) => format!("from explicitly specified {}", f),
                    None => format!("from default {}", DEFAULT_HOSTOS_FIREWALL_JSON_PATH),
                },
                config
            );
            println!(
                "{}",
                match config {
                    Some(c) => c.as_nftables(&firewall::FirewallRuleDestination::HostOS),
                    None => "".to_string(),
                },
            );
            Ok(())
        }
        None => Err(anyhow!(
            "No subcommand specified. Run with '--help' for subcommands"
        )),
    }
}
