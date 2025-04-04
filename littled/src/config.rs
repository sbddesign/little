use std::path::{Path, PathBuf};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use names;

const DEFAULT_ALIAS_PREFIX: &str = "little-node";
const DEFAULT_LIGHTNING_PORT: u16 = 9735;
const DEFAULT_GRPC_PORT: u16 = 50051;
const DEFAULT_HTTP_PORT: u16 = 3030;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub node_alias: String,
    pub lightning_port: u16,
    pub grpc_port: u16,
    pub http_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        let mut generator = names::Generator::default();
        let alias = generator.next().unwrap_or_else(|| DEFAULT_ALIAS_PREFIX.to_string());
        
        Config {
            node_alias: alias,
            lightning_port: DEFAULT_LIGHTNING_PORT,
            grpc_port: DEFAULT_GRPC_PORT,
            http_port: DEFAULT_HTTP_PORT,
        }
    }
}

pub fn get_default_data_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".little")
}

pub fn load_config(data_dir: &Path) -> Result<Config, String> {
    let config_path = data_dir.join("little.conf");
    
    if !config_path.exists() {
        // Create default config if it doesn't exist
        let config = Config {
            node_alias: names::Generator::default().next().unwrap(),
            lightning_port: 9735,
            grpc_port: 50051,
            http_port: 3030,
        };
        
        // Save the default config
        save_config(data_dir, &config)
            .map_err(|e| format!("Failed to save default config: {}", e))?;
        Ok(config)
    } else {
        // Load existing config
        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config file: {}", e))?;
        
        let mut config = Config::default();
        
        for line in config_str.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('\'');
                
                match key {
                    "NODE_ALIAS" => config.node_alias = value.to_string(),
                    "LIGHTNING_PORT" => config.lightning_port = value.parse()
                        .map_err(|e| format!("Invalid LIGHTNING_PORT value: {}", e))?,
                    "GRPC_PORT" => config.grpc_port = value.parse()
                        .map_err(|e| format!("Invalid GRPC_PORT value: {}", e))?,
                    "HTTP_PORT" => config.http_port = value.parse()
                        .map_err(|e| format!("Invalid HTTP_PORT value: {}", e))?,
                    _ => continue,
                }
            }
        }
        
        // Validate required fields
        if config.node_alias.is_empty() {
            return Err("Config file is missing NODE_ALIAS".to_string());
        }
        if config.lightning_port == 0 {
            return Err("Config file is missing LIGHTNING_PORT".to_string());
        }
        if config.grpc_port == 0 {
            return Err("Config file is missing GRPC_PORT".to_string());
        }
        if config.http_port == 0 {
            return Err("Config file is missing HTTP_PORT".to_string());
        }
        
        Ok(config)
    }
}

pub fn save_config(data_dir: &Path, config: &Config) -> Result<(), Box<dyn Error>> {
    let config_path = data_dir.join("little.conf");
    
    // Create the data directory if it doesn't exist
    std::fs::create_dir_all(data_dir)?;

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(config_path)?;

    writeln!(file, "NODE_ALIAS='{}'", config.node_alias)?;
    writeln!(file, "LIGHTNING_PORT={}", config.lightning_port)?;
    writeln!(file, "GRPC_PORT={}", config.grpc_port)?;
    writeln!(file, "HTTP_PORT={}", config.http_port)?;

    Ok(())
} 