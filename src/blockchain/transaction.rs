use serde::{Deserialize, Serialize};

use super::address::Address;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub sender: Address,
    pub recipient: Address,
    pub amount: u64,
    pub kind: TransactionKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionKind {
    Transfer,
    AuctionWon { auction: Address },
}

impl Transaction {
    #[allow(dead_code)]
    pub fn transfer(sender: Address, recipient: Address, amount: u64) -> Self {
        Self {
            sender,
            recipient,
            amount,
            kind: TransactionKind::Transfer,
        }
    }

    pub fn auction_won(
        auction: Address,
        seller: Address,
        winner: Address,
        winning_bid: u64,
    ) -> Self {
        Self {
            sender: winner,
            recipient: seller,
            amount: winning_bid,
            kind: TransactionKind::AuctionWon { auction },
        }
    }
}
