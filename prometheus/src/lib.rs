mod bank_metrics;
pub mod banks_with_commitments;
mod cluster_metrics;
pub mod identity_info;
mod snapshot_metrics;
mod utils;

use banks_with_commitments::BanksWithCommitments;
use identity_info::{map_vote_identity_to_info, IdentityInfoMap};
use solana_gossip::cluster_info::ClusterInfo;
use solana_runtime::{
    bank_forks::BankForks, commitment::BlockCommitmentCache, snapshot_config::SnapshotConfig,
};
use solana_sdk::pubkey::Pubkey;
use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
    thread,
};

#[derive(Clone, Copy)]
pub struct Lamports(pub u64);

pub struct PrometheusMetrics {
    bank_forks: Arc<RwLock<BankForks>>,
    block_commitment_cache: Arc<RwLock<BlockCommitmentCache>>,
    cluster_info: Arc<ClusterInfo>,
    vote_accounts: Arc<HashSet<Pubkey>>,
    snapshot_config: Option<SnapshotConfig>,
    /// Initialized based on vote_accounts. Maps identity
    /// pubkey associated with the vote account to the validator info.
    /// Since loading accounts takes a lot of time, we initialize it in a
    /// separate thread, hence the RwLock - to set the data later from a
    /// different thread.
    identity_info_map: RwLock<Option<IdentityInfoMap>>,
}

impl PrometheusMetrics {
    pub fn new(
        bank_forks: Arc<RwLock<BankForks>>,
        block_commitment_cache: Arc<RwLock<BlockCommitmentCache>>,
        cluster_info: Arc<ClusterInfo>,
        vote_accounts: Arc<HashSet<Pubkey>>,
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

        let prom_metrics_clone = prom_metrics.clone();
        thread::spawn(move || {
            // TODO: This can panic, we should handle it better
            let identity_info_map = map_vote_identity_to_info(&bank_forks, &vote_accounts);
            prom_metrics_clone
                .identity_info_map
                .write()
                .unwrap()
                .replace(identity_info_map);
        });

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
