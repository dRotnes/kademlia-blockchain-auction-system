mod config;
pub mod logger;
pub mod execution;
pub mod termination;
pub mod context;
pub mod crypto_own;

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
 * Calculates the XOR distance metric between two node ids.
 */
pub fn calculate_distance(id1: U256, id2: U256) -> U256 {
    id1 ^id2
}

/**
 * Formats a U256 as a hex string.
 */
pub fn format_as_hex_string(number: U256) -> String {
    format!("{:x}", number)
}