extern crate dotenv;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

// type StringVec = Vec<String>;

// Encapsulates configuration values to be used across the application
// It ensures correct typing and that at least they will have a default value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // Networking settings
    pub port: u32,

    // Kademlia settings
    pub bootstrap_peer_ip: String,
    pub bootstrap_peer_port: u32,
    pub max_kbucket_entries: usize,
    pub k_value: usize,

    // Keys
    pub private_key: Vec<u8>,
    pub public_key: Vec<u8>,
    pub private_key_path: String,
    pub public_key_path: String,
    pub data_dir: String,

    // Extra settings
    pub peer_sync_ms: u64,
    pub challenge_difficulty: u32,
    pub n_max_retries: u8,
    pub consensus_mode: String,
    pub reputation_threshold: u64,
    pub automation_enabled: bool,
    pub automation_interval_ms: u64,
    pub auction_duration_ms: i64,
}

impl Config {
    // Parse and return configuration values from environment variables
    pub fn read() -> Config {
        dotenv().ok();

        let args: Vec<String> = env::args().collect();
        let port_from_args = args
            .iter()
            .position(|arg| arg == "--port")
            .and_then(|index| args.get(index + 1))
            .and_then(|port_str| port_str.parse::<u32>().ok());

        let port = match port_from_args {
            Some(port) => port,
            None => {
                eprintln!("Error: Missing required argument '--port <number>'");
                eprintln!("Usage: cargo run -- --port <PORT_NUMBER>");
                std::process::exit(1);
            }
        };

        let key_dir = PathBuf::from("./keys").join(port.to_string());
        let public_key_path = key_dir.join("public_key.pem").to_string_lossy().to_string();
        let private_key_path = key_dir
            .join("private_key.pem")
            .to_string_lossy()
            .to_string();
        let data_dir = PathBuf::from("./data")
            .join(port.to_string())
            .to_string_lossy()
            .to_string();

        Config {
            port,

            // Kademlia settings
            bootstrap_peer_ip: Config::read_envvar::<String>(
                "BOOTSTRAP_PEER_IP",
                String::from("127.0.0.1"),
            ),
            bootstrap_peer_port: Config::read_envvar::<u32>("BOOTSTRAP_PEER_PORT", 8000),
            max_kbucket_entries: Config::read_envvar::<usize>("MAX_N_KBUCKET_ENTRIES", 2),
            k_value: Config::read_envvar::<usize>("K_VALUE", 2),

            // Private key
            private_key: Config::read_key(&private_key_path),
            // Public key
            public_key: Config::read_key(&public_key_path),
            private_key_path,
            public_key_path,
            data_dir,

            // Extra settings
            peer_sync_ms: Config::read_envvar::<u64>("PEER_SYNC_MS", 10000),
            challenge_difficulty: Config::read_envvar::<u32>("CHALLENGE_DIFFICULTY", 5),
            n_max_retries: Config::read_envvar::<u8>("N_MAX_RETRIES", 3),
            consensus_mode: Config::read_envvar::<String>("CONSENSUS_MODE", "pow".to_string()),
            reputation_threshold: Config::read_envvar::<u64>("REPUTATION_THRESHOLD", 1),
            automation_enabled: Config::read_envvar::<bool>("AUTOMATION_ENABLED", false),
            automation_interval_ms: Config::read_envvar::<u64>("AUTOMATION_INTERVAL_MS", 5000),
            auction_duration_ms: Config::read_envvar::<i64>("AUCTION_DURATION_MS", 30000),
        }
    }

    // Parses a singular value from a environment variable, accepting a default value if missing
    fn read_envvar<T: FromStr>(key: &str, default_value: T) -> T {
        match env::var(key) {
            Ok(val) => val.parse::<T>().unwrap_or(default_value),
            Err(_e) => default_value,
        }
    }

    // Reads a PEM file and returns the raw DER bytes.
    pub fn read_key(file_path: &str) -> Vec<u8> {
        let pem_content = fs::read_to_string(file_path).unwrap_or_else(|_| {
            warn!("Warning: Failed to read key from {}", file_path);
            String::new()
        });

        // Strip PEM headers and footers
        let base64_data = pem_content
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect::<String>();

        match BASE64_STANDARD.decode(base64_data) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!("Warning: Failed to decode base64 key: {:?}", e);
                Vec::new()
            }
        }
    }

    // Parses a multiple value (Vec) from a environment variable, accepting a default value if missing
    // fn read_vec_envvar(key: &str, separator: &str, default_value: StringVec) -> StringVec {
    //     match env::var(key) {
    //         Ok(val) => val
    //             .trim()
    //             .split_terminator(separator)
    //             .map(str::to_string)
    //             .collect(),
    //         Err(_e) => default_value,
    //     }
    // }
}
