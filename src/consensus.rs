use std::collections::HashMap;

use crate::{
    blockchain::{address::Address, block::Block, transaction::TransactionKind},
    utils::Config,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsensusMode {
    ProofOfWork,
    ProofOfReputation,
}

impl ConsensusMode {
    pub fn from_config(config: &Config) -> Self {
        match config.consensus_mode.as_str() {
            "por" | "proof-of-reputation" => ConsensusMode::ProofOfReputation,
            _ => ConsensusMode::ProofOfWork,
        }
    }
}

pub fn reputation_scores(ledger: &[Block]) -> HashMap<Address, u64> {
    let mut scores = HashMap::new();

    for block in ledger {
        for transaction in &block.transactions {
            if matches!(transaction.kind, TransactionKind::AuctionWon { .. }) {
                *scores.entry(transaction.sender.clone()).or_insert(0) += 1;
                *scores.entry(transaction.recipient.clone()).or_insert(0) += 1;
            }
        }
    }

    scores
}

pub fn has_reputation_to_commit(config: &Config, ledger: &[Block], node_id: &Address) -> bool {
    match ConsensusMode::from_config(config) {
        ConsensusMode::ProofOfWork => true,
        ConsensusMode::ProofOfReputation => {
            reputation_scores(ledger)
                .get(node_id)
                .copied()
                .unwrap_or_default()
                >= config.reputation_threshold
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blockchain::{address::Address, block::Block, transaction::Transaction},
        utils::Config,
    };
    use ethereum_types::U256;
    use std::str::FromStr;

    fn config(mode: &str) -> Config {
        Config {
            port: 8000,
            bootstrap_peer_ip: "127.0.0.1".to_string(),
            bootstrap_peer_port: 8000,
            max_kbucket_entries: 2,
            k_value: 2,
            private_key: vec![],
            public_key: vec![],
            private_key_path: "./target/test/private_key.pem".to_string(),
            public_key_path: "./target/test/public_key.pem".to_string(),
            data_dir: "./target/test/consensus".to_string(),
            peer_sync_ms: 10,
            challenge_difficulty: 4,
            n_max_retries: 1,
            consensus_mode: mode.to_string(),
            reputation_threshold: 1,
            automation_enabled: false,
            automation_interval_ms: 1000,
            auction_duration_ms: 1000,
        }
    }

    #[test]
    fn proof_of_reputation_requires_a_score() {
        let auction =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000003")
                .unwrap();
        let seller =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let winner =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let outsider =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000004")
                .unwrap();
        let transaction = Transaction::auction_won(auction, seller, winner.clone(), 30);
        let ledger = vec![Block::new(1, 0, U256::zero(), vec![transaction])];

        assert!(has_reputation_to_commit(&config("por"), &ledger, &winner));
        assert!(!has_reputation_to_commit(
            &config("por"),
            &ledger,
            &outsider
        ));
        assert!(has_reputation_to_commit(&config("pow"), &[], &outsider));
    }
}
