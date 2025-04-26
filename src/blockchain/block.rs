use chrono::prelude::*;
use ethereum_types::U256;
use serde::{Deserialize, Serialize};

use crate::utils::crypto_own::hash_data;

use super::transaction::Transaction;

// Represents a block in a blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub index: u64,
    pub timestamp: i64,
    pub nonce: u64,
    pub previous_hash: U256,
    pub hash: U256,
    pub transactions: Vec<Transaction>,
}

impl Block {
    
    pub fn new(
        index: u64,
        nonce: u64,
        previous_hash: U256,
        transactions: Vec<Transaction>,
    ) -> Block {
        let mut block = Block {
            index,
            timestamp: Utc::now().timestamp_millis(),
            nonce,
            previous_hash,
            hash: U256::default(),
            transactions,
        };
        block.hash = block.calculate_hash();

        block
    }

    // Calculate the hash value of the block
    pub fn calculate_hash(&self) -> U256 {
        
        let mut hashable_data = self.clone();
        hashable_data.hash = U256::default();
        let serialized = serde_json::to_string(&hashable_data).unwrap();

        hash_data(serialized)
    }
}
