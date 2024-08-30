use serde::{Deserialize, Serialize};
use clap::Subcommand;
      
#[derive(Debug, Clone, Deserialize, Serialize, Subcommand)]
pub enum Command {
    Start { name: Option<String> },
    Stop,
    GetInfo,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetInfoResponse {
    pub alias: String,
    pub public_key: String,
}

