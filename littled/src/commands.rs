use serde::{Deserialize, Serialize};
use clap::Subcommand;
      
#[derive(Debug, Clone, Deserialize, Serialize, Subcommand)]
pub enum Command {
    Start {name: Option<String> },
    Stop,
    GetInfo
}

