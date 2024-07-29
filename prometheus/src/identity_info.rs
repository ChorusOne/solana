use solana_sdk::pubkey::Pubkey;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;

use crate::PrometheusMetricsConfig;

/// ValidatorInfo represents selected fields from the config account data.
#[derive(Debug, Default, Deserialize, Clone, Eq, PartialEq)]
pub struct ValidatorInfo {
    pub name: String,
}

pub type IdentityInfoMap = HashMap<Pubkey, ValidatorInfo>;

impl TryFrom<PrometheusMetricsConfig> for IdentityInfoMap {
    type Error = solana_sdk::pubkey::ParsePubkeyError;

    fn try_from(value: PrometheusMetricsConfig) -> std::result::Result<Self, Self::Error> {
        value
            .monitor_identity_accounts
            .into_iter()
            .map(|acc| {
                let pubkey = Pubkey::from_str(&acc.identity_pubkey)?;
                Ok((
                    pubkey,
                    ValidatorInfo {
                        name: acc.validator_name,
                    },
                ))
            }).collect()
    }
}
