use serde::{Deserialize, Serialize};
use clap::Subcommand;
      
#[derive(Debug, Clone, Deserialize, Serialize, Subcommand)]
#[command(rename_all = "lowercase")]
pub enum Command {
    Start {
        /// Optional name for the node
        name: Option<String>,
        /// Port for the Lightning Network P2P protocol (default: 9735)
        #[arg(long = "lightningport", default_value = "9735")]
        lightning_port: u16,
        /// Port for the gRPC API (default: 50051)
        #[arg(long = "grpcport", default_value = "50051")]
        grpc_port: u16,
        /// Port for the HTTP API (default: 3030)
        #[arg(long = "httpport", default_value = "3030")]
        http_port: u16,
        /// Data directory path (default: ~/.little)
        #[arg(long = "datadir")]
        data_dir: Option<String>,
    },
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
    /// List all created offers
    ListOffers,
    /// List all channels
    ListChannels,
    /// Open a channel with a peer
    OpenChannel {
        /// The node ID of the peer to open a channel with
        #[arg(long)]
        peer_pubkey: String,
        /// The address of the peer (e.g. 127.0.0.1:9735)
        #[arg(long)]
        address: String,
        /// The amount in satoshis to fund the channel with
        #[arg(long)]
        amount_sats: u64,
        /// The target number of blocks for the funding transaction to confirm in
        #[arg(long, default_value = "6")]
        target_conf: u32,
        /// Whether to push some initial funds to the counterparty (not supported by LDK Node)
        #[arg(long, default_value = "0")]
        push_amount_sats: u64,
        /// Whether to announce the channel on the network
        #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
        announced: bool,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StoredOfferDetails {
    pub offer_string: String,
    pub amount_sats: Option<u64>,
    pub description: String,
    pub created_at: u64,  // Unix timestamp
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChannelDetailsResponse {
    pub channel_id: String,
    pub counterparty_node_id: String,
    pub funding_txo: Option<String>,
    pub channel_value_sats: u64,
    pub unspendable_punishment_reserve: Option<u64>,
    pub user_channel_id: String,
    pub outbound_capacity_msat: u64,
    pub inbound_capacity_msat: u64,
    pub confirmations_required: Option<u32>,
    pub confirmations: Option<u32>,
    pub is_outbound: bool,
    pub is_channel_ready: bool,
    pub is_usable: bool,
    pub cltv_expiry_delta: Option<u16>,
    pub announced: bool,
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

