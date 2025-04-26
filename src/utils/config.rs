extern crate dotenv;

use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use std::fs;
use std::env;
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
    pub private_key: String,
    pub public_key: String,
    
    // Extra settings
    pub peer_sync_ms: u64,
    pub challenge_difficulty: u32,
    pub n_max_retries: u8,
}

impl Config {
    // Parse and return configuration values from environment variables
    pub fn read() -> Config {
        dotenv().ok();

        let args: Vec<String> = env::args().collect();
        let port_from_args = args.iter()
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

        // TO DO: CHANGE THIS TO ONLY ./keys/... WHEN FINISHED WITH LOCAL DEVELOPMENT.
        let pb_key_file: &str = if port == 8000 {"./keys/1/public_key.pem"} else if port == 8001 {"./keys/2/public_key.pem"} else if port == 8002 {"./keys/3/public_key.pem"} else {"./keys/4/public_key.pem"}; 
        let pv_key_file: &str = if port == 8000 {"./keys/1/private_key.pem"} else if port == 8001 {"./keys/2/private_key.pem"} else if port == 8002 {"./keys/3/private_key.pem"} else {"./keys/4/private_key.pem"};

        Config {
            port,

            // Kademlia settings
            bootstrap_peer_ip: Config::read_envvar::<String>("BOOTSTRAP_PEER_IP", String::from("127.0.0.1")),
            bootstrap_peer_port: Config::read_envvar::<u32>("BOOTSTRAP_PEER_PORT", 8000),
            max_kbucket_entries: Config::read_envvar::<usize>("MAX_N_KBUCKET_ENTRIES", 2),
            k_value: Config::read_envvar::<usize>("K_VALUE", 2),
            
            // Private key
            private_key: Config::read_key(pv_key_file),
            // Public key
            public_key:  Config::read_key(pb_key_file),
            
            // Extra settings
            peer_sync_ms: Config::read_envvar::<u64>("PEER_SYNC_MS", 10000),
            challenge_difficulty: Config::read_envvar::<u32>("CHALLENGE_DIFFICULTY", 5),
            n_max_retries: Config::read_envvar::<u8>("N_MAX_RETRIES", 3),
        }
    }

    // Parses a singular value from a environment variable, accepting a default value if missing
    fn read_envvar<T: FromStr>(key: &str, default_value: T) -> T {
        match env::var(key) {
            Ok(val) => val.parse::<T>().unwrap_or(default_value),
            Err(_e) => default_value,
        }
    }

    // Reads the private key
    fn read_key(file_path: &str) -> String {
        fs::read_to_string(file_path).unwrap_or_else(|_| {
            warn!("Warning: Failed to read private key from {}", file_path);
            String::default()
        })
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