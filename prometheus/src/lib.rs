mod bank_metrics;
pub mod banks_with_commitments;
mod cluster_metrics;
mod snapshot_metrics;
mod utils;

use banks_with_commitments::BanksWithCommitments;
use log::info;
use serde::Deserialize;
use solana_gossip::cluster_info::ClusterInfo;
use solana_runtime::{
    bank_forks::BankForks, commitment::BlockCommitmentCache, snapshot_config::SnapshotConfig,
};
use solana_sdk::pubkey::Pubkey;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, RwLock},
};

#[derive(Clone, Copy)]
pub struct Lamports(pub u64);

#[derive(Clone, Debug, Deserialize)]
pub struct PrometheusMetricsConfig {
    pub monitor_vote_accounts: Option<Vec<MonitorAccount>>,
    pub monitor_accounts_balance: Option<Vec<MonitorAccount>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MonitorAccount {
    /// Base58 encoded account pubkey
    pub pubkey: String,
    /// Name associated with the account. E.g. for vote accounts, this is the
    /// validator name.
    pub name: Option<String>,
}

/// ValidatorInfo represents selected fields from the config account data.
#[derive(Debug, Default, Deserialize, Clone, Eq, PartialEq)]
pub struct ValidatorInfo {
    pub name: String,
}

pub type ValidatorInfoMap = HashMap<Pubkey, ValidatorInfo>;

pub struct PrometheusMetrics {
    bank_forks: Arc<RwLock<BankForks>>,
    block_commitment_cache: Arc<RwLock<BlockCommitmentCache>>,
    cluster_info: Arc<ClusterInfo>,
    vote_accounts: Arc<HashSet<Pubkey>>,
    accounts_to_monitor_balance: Arc<HashSet<Pubkey>>,
    snapshot_config: Option<SnapshotConfig>,
    /// Initialized from accounts_config_file.
    /// Maps vote pubkey to the validator info.
    validator_info_map: ValidatorInfoMap,
}

impl PrometheusMetrics {
    pub fn new(
        bank_forks: Arc<RwLock<BankForks>>,
        block_commitment_cache: Arc<RwLock<BlockCommitmentCache>>,
        cluster_info: Arc<ClusterInfo>,
        accounts_config_file: Option<PathBuf>,
        default_vote_account_to_monitor: Option<Pubkey>,
        snapshot_config: Option<SnapshotConfig>,
    ) -> Arc<Self> {
        let mut vote_accounts = HashSet::new();
        let mut accounts_to_monitor_balance = HashSet::new();
        let mut validator_info_map = HashMap::new();

        if let Some(default_vote_account_to_monitor) = default_vote_account_to_monitor {
            vote_accounts.insert(default_vote_account_to_monitor);
        }

        // We read and parse config file here instead of doing it earlier, so
        // that we do not need to import types from this library in the main.
        if let Some(accounts_config_file) = accounts_config_file {
            info!("Monitor accounts config file provided, reading accounts to monitor from it...");

            // We use yaml to be consistent with the rest of the config files.
            let file = File::open(accounts_config_file)
                .expect("Unable to open monitor accounts config file");
            // At this point, it is easier for us to crash the application here
            // than propagating the error.
            let config = serde_yaml::from_reader::<_, PrometheusMetricsConfig>(file).expect(
                "Unable to deserialize prometheus metrics config from the monitor accounts config file",
            );

            if let Some(monitor_vote_accounts) = &config.monitor_vote_accounts {
                monitor_vote_accounts.iter().for_each(|acc| {
                    let pubkey = Pubkey::from_str(&acc.pubkey)
                        .expect("Unable to parse pubkey from the monitor accounts config file");
                    if let Some(name) = &acc.name {
                        validator_info_map.insert(pubkey, ValidatorInfo { name: name.clone() });
                    }
                    vote_accounts.insert(pubkey);
                });
            }

            if let Some(monitor_accounts_balance) = &config.monitor_accounts_balance {
                monitor_accounts_balance.iter().for_each(|acc| {
                    accounts_to_monitor_balance
                        .insert(Pubkey::from_str(&acc.pubkey).expect(
                            "Unable to parse pubkey from the monitor accounts config file",
                        ));
                });
            }
        };

        let prom_metrics = Self {
            bank_forks: bank_forks.clone(),
            block_commitment_cache,
            cluster_info,
            vote_accounts: Arc::new(vote_accounts),
            accounts_to_monitor_balance: Arc::new(accounts_to_monitor_balance),
            validator_info_map,
            snapshot_config,
        };
        Arc::new(prom_metrics)
    }

    pub fn render_prometheus(&self) -> Vec<u8> {
        let banks_with_comm =
            BanksWithCommitments::new(&self.bank_forks, &self.block_commitment_cache);

        // There are 3 levels of commitment for a bank:
        // - finalized: most recent block *confirmed* by supermajority of the
        // cluster.
        // - confirmed: most recent block that has been *voted* on by supermajority
        // of the cluster.
        // - processed: most recent block.
        let mut out: Vec<u8> = Vec::new();
        bank_metrics::write_bank_metrics(&banks_with_comm, &mut out).expect("IO error");

        cluster_metrics::write_node_metrics(&banks_with_comm, &self.cluster_info, &mut out)
            .expect("IO error");

        cluster_metrics::write_accounts_metrics(
            &banks_with_comm,
            &self.vote_accounts,
            &self.accounts_to_monitor_balance,
            &self.validator_info_map,
            &mut out,
        )
        .expect("IO error");

        if let Some(snapshot_config) = self.snapshot_config.as_ref() {
            snapshot_metrics::write_snapshot_metrics(snapshot_config, &mut out).expect("IO error");
        }
        out
    }
}
