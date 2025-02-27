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
    /// Connect to a peer using the format node_id@address (e.g. 028374...@127.0.0.1:9735)
    Connect {
        /// The peer string in format node_id@address
        #[arg(value_parser = parse_peer_string)]
        peer: PeerString,
    },
    /// List all connected peers
    ListPeers,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PeerString {
    pub node_id: String,
    pub address: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PeerDetailsResponse {
    pub node_id: String,
    pub address: String,
    pub is_persisted: bool,
}

pub fn parse_peer_string(s: &str) -> Result<PeerString, String> {
    let parts: Vec<&str> = s.split('@').collect();
    if parts.len() != 2 {
        return Err("Peer string must be in format node_id@address".to_string());
    }
    Ok(PeerString {
        node_id: parts[0].to_string(),
        address: parts[1].to_string(),
    })
}

