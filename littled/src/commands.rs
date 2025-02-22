use serde::{Deserialize, Serialize};
use clap::Subcommand;
      
#[derive(Debug, Clone, Deserialize, Serialize, Subcommand)]
#[command(rename_all = "lowercase")]
pub enum Command {
    Start { name: Option<String> },
    Stop,
    GetInfo,
    GetAddress,
    ListBalances,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetInfoResponse {
    pub alias: String,
    pub public_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetAddressResponse {
    pub address: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListBalancesResponse {
    pub total_onchain_balance_sats: u64,
    pub total_lightning_balance_sats: u64,
    pub spendable_onchain_balance_sats: u64,
}

