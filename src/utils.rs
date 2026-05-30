mod config;
pub mod context;
pub mod crypto_own;
pub mod execution;
pub mod logger;
pub mod termination;

pub use config::Config;

use crypto_own::hash_data;
use ethereum_types::U256;
use rand::{thread_rng, Rng};

/**
 * Resolves a proof of work challenge.
 * Given an input and difficulty, finds a nonce that produces a hash with enough leading zeros.
 */
pub fn proof_of_work(input: &str, difficulty: u32) -> u64 {
    let mut nonce: u64 = 0;

    loop {
        let data_to_hash = format!("{}{}", input, nonce);
        let solved_hash = hash_data(&data_to_hash);

        if solved_hash.leading_zeros() >= difficulty {
            return nonce;
        }

        nonce += 1;
    }
}

/**
 * Generates a challenge. (Random U256)
 */
pub fn generate_challenge() -> U256 {
    let mut rng = thread_rng();

    // Generate a random 256-bit number
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes);

    U256::from(&bytes)
}

/**
 * Formats a U256 as a hex string.
 */
pub fn format_as_hex_string(number: U256) -> String {
    format!("{:064x}", number)
}

/**
 * Generates an url based on node ip and port.
 */
pub fn generate_url(node_ip: &str, node_port: u32) -> String {
    format!("http://{}:{}", node_ip, node_port)
}
