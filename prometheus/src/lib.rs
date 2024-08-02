mod bank_metrics;
pub mod banks_with_commitments;
mod cluster_metrics;
pub mod identity_info;
mod snapshot_metrics;
mod utils;

use banks_with_commitments::BanksWithCommitments;
use identity_info::IdentityInfoMap;
use log::info;
use serde::Deserialize;
use solana_gossip::cluster_info::ClusterInfo;
use solana_runtime::{
    bank_forks::BankForks, commitment::BlockCommitmentCache, snapshot_config::SnapshotConfig,
};
use solana_sdk::pubkey::Pubkey;
use std::{
    collections::HashSet,
    fs::File,
    path::PathBuf,
    sync::{Arc, RwLock},
};

#[derive(Clone, Copy)]
pub struct Lamports(pub u64);

#[derive(Clone, Debug, Deserialize)]
pub struct PrometheusMetricsConfig {
    pub monitor_identity_accounts: Vec<MonitorIdentityAccount>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MonitorIdentityAccount {
    /// Base58 encoded identity pubkey
    pub identity_pubkey: String,
    pub validator_name: String,
}

pub struct PrometheusMetrics {
    bank_forks: Arc<RwLock<BankForks>>,
    block_commitment_cache: Arc<RwLock<BlockCommitmentCache>>,
    cluster_info: Arc<ClusterInfo>,
    vote_accounts: Arc<HashSet<Pubkey>>,
    accounts_to_monitor_balance: Arc<HashSet<Pubkey>>,
    snapshot_config: Option<SnapshotConfig>,
    /// Initialized based on identity_accounts_file.
    /// Maps identity pubkey to the validator info.
    identity_info_map: Option<IdentityInfoMap>,
}

impl PrometheusMetrics {
    pub fn new(
        bank_forks: Arc<RwLock<BankForks>>,
        block_commitment_cache: Arc<RwLock<BlockCommitmentCache>>,
        cluster_info: Arc<ClusterInfo>,
        vote_accounts: Arc<HashSet<Pubkey>>,
        accounts_to_monitor_balance: Arc<HashSet<Pubkey>>,
        identity_accounts_file: Option<PathBuf>,
        snapshot_config: Option<SnapshotConfig>,
    ) -> Arc<Self> {
        // We read and parse config file here instead of doing it earlier, so
        // that we do not need to import types from this library in the main.
        let identity_map = identity_accounts_file.map(|identity_accounts_file| {
            info!("Identity accounts file provided, reading accounts to monitor from it...");

            // We use yaml to be consistent with the rest of the config files.
            let file =
                File::open(identity_accounts_file).expect("Unable to open identity accounts file");
            // At this point, it is easier for us to crash the application here
            // than propagating the error.
            let config = serde_yaml::from_reader::<_, PrometheusMetricsConfig>(file).expect(
                "Unable to deserialize prometheus metrics config from identity accounts file",
            );
            config
                .try_into()
                .expect("Unable to parse config to identity accounts map")
        });

        let prom_metrics = Self {
            bank_forks: bank_forks.clone(),
            block_commitment_cache,
            cluster_info,
            vote_accounts: vote_accounts.clone(),
            accounts_to_monitor_balance: accounts_to_monitor_balance.clone(),
            identity_info_map: identity_map,
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
            &self.identity_info_map,
            &mut out,
        )
        .expect("IO error");

        if let Some(snapshot_config) = self.snapshot_config.as_ref() {
            snapshot_metrics::write_snapshot_metrics(snapshot_config, &mut out).expect("IO error");
        }
        out
    }
}
