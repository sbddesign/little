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
    GetOffer { 
        /// Amount in satoshis (optional - if not provided, creates a variable-amount offer)
        #[arg(long)]
        amount_sats: Option<u64>,
        /// Description of the payment request
        #[arg(long)]
        description: Option<String>,
    },
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetOfferResponse {
    pub offer_string: String,
}

