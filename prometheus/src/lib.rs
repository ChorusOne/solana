mod bank_metrics;
pub mod banks_with_commitments;
mod cluster_metrics;
pub mod identity_info;
mod snapshot_metrics;
mod utils;

use banks_with_commitments::BanksWithCommitments;
use identity_info::{map_vote_identity_to_info, IdentityInfoMap};
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
    thread,
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
    snapshot_config: Option<SnapshotConfig>,
    /// Initialized based on identity_accounts_file or vote_accounts.
    /// Maps identity pubkey associated with the vote account to the validator info.
    /// Since loading accounts takes a lot of time and we faced a lot of issues
    /// with it, we prefer using identity_accounts_file if provided. Otherwise, we 
    /// initialize it in a separate thread, hence the RwLock - to set the data later
    /// from a different thread.
    identity_info_map: RwLock<Option<IdentityInfoMap>>,
}

impl PrometheusMetrics {
    /// Create a new instance of `PrometheusMetrics`
    /// `identity_accounts_file` is a path to a file containing a list of identity
    /// accounts to monitor. If `identity_accounts_file` if provided, vote_accounts
    /// will be ignored.
    pub fn new(
        bank_forks: Arc<RwLock<BankForks>>,
        block_commitment_cache: Arc<RwLock<BlockCommitmentCache>>,
        cluster_info: Arc<ClusterInfo>,
        vote_accounts: Arc<HashSet<Pubkey>>,
        identity_accounts_file: Option<PathBuf>,
        snapshot_config: Option<SnapshotConfig>,
    ) -> Arc<Self> {
        let prom_metrics = Self {
            bank_forks: bank_forks.clone(),
            block_commitment_cache,
            cluster_info,
            vote_accounts: vote_accounts.clone(),
            identity_info_map: RwLock::new(None),
            snapshot_config,
        };
        let prom_metrics = Arc::new(prom_metrics);

        // We read and parse config file here instead of doing it earlier, so
        // that we do not need to import types from this library in the main.
        if let Some(identity_accounts_file) = identity_accounts_file {
            info!("Identity accounts file provided, reading accounts to monitor from it...");

            // We use yaml to be consistent with the rest of the config files.
            let file =
                File::open(identity_accounts_file).expect("Unable to open identity accounts file");
            // At this point, it is easier for us to crash the application here
            // than propagating the error.
            let config = serde_yaml::from_reader::<_, PrometheusMetricsConfig>(file).expect(
                "Unable to deserialize prometheus metrics config from identity accounts file",
            );
            prom_metrics
                .identity_info_map
                .write()
                .unwrap()
                .replace(config.try_into().expect("Unable to parse config to identity accounts map"));

            return prom_metrics;
        }
 
        // Initialize this way only if identity_accounts_file is not provided.
        // let prom_metrics_clone = prom_metrics.clone();
        // // TODO: If it works for us with the file setup, let's consider removing this altogether.
        // thread::spawn(move || {
        //     info!("Initializing identity info map...");
        //     // TODO: This can panic, we should handle it better
        //     let identity_info_map = map_vote_identity_to_info(&bank_forks, &vote_accounts);
        //     info!("Identity info map initialized. Enabling accounts metrics...");
        //     prom_metrics_clone
        //         .identity_info_map
        //         .write()
        //         .unwrap()
        //         .replace(identity_info_map);
        // });

        prom_metrics
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

        let identity_map = &self.identity_info_map.read().unwrap().clone();
        if let Some(identity_info_map) = identity_map {
            cluster_metrics::write_accounts_metrics(
                &banks_with_comm,
                &self.vote_accounts,
                identity_info_map,
                &mut out,
            )
            .expect("IO error");
        }
        if let Some(snapshot_config) = self.snapshot_config.as_ref() {
            snapshot_metrics::write_snapshot_metrics(snapshot_config, &mut out).expect("IO error");
        }
        out
    }
}
