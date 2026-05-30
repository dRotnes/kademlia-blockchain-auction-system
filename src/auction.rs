use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::g_rpc::kademlia;
use crate::utils::format_as_hex_string;
use crate::{blockchain::address::Address, utils::crypto_own::hash_data};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Auction {
    pub key: Address,
    pub object: String,
    pub initial_value: u64,
    pub seller: Address,
    pub bids: Vec<Bid>,
    pub status: AuctionStatus,
    pub created_at: i64,
    pub ends_at: i64,
    pub winner: Option<Address>,
    pub winning_bid: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuctionStatus {
    Open,
    Closed,
}

impl AuctionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuctionStatus::Open => "open",
            AuctionStatus::Closed => "closed",
        }
    }
}

impl FromStr for AuctionStatus {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "open" | "" => Ok(AuctionStatus::Open),
            "closed" => Ok(AuctionStatus::Closed),
            status => Err(anyhow!("Unknown auction status '{}'", status)),
        }
    }
}

impl Auction {
    /// Create a new auction and derive its DHT key from immutable auction data.
    ///
    /// The key intentionally excludes bids so the auction can be found by the
    /// same address as bids are appended.
    pub fn new(object: String, initial_value: u64, seller: &Address) -> Auction {
        Self::new_with_duration(object, initial_value, seller, 30_000)
    }

    pub fn new_with_duration(
        object: String,
        initial_value: u64,
        seller: &Address,
        duration_ms: i64,
    ) -> Auction {
        let now = Utc::now().timestamp_millis();
        let mut auction = Auction {
            key: Address::default(),
            object,
            initial_value,
            seller: seller.clone(),
            bids: Vec::new(),
            status: AuctionStatus::Open,
            created_at: now,
            ends_at: now + duration_ms,
            winner: None,
            winning_bid: None,
        };

        auction.key = auction.generate_key();

        auction
    }

    fn generate_key(&self) -> Address {
        let data_to_hash = format!(
            "{}{}{}",
            self.object,
            self.initial_value,
            self.seller.to_string()
        );
        Address::from_str(&format_as_hex_string(hash_data(&data_to_hash))).unwrap()
    }

    pub fn highest_bid_amount(&self) -> u64 {
        self.bids
            .iter()
            .map(|bid| bid.amount)
            .max()
            .unwrap_or(self.initial_value)
    }

    /// Validate and append a bid. This is deliberately local/domain-only:
    /// network authenticity is handled by signed RPCs, while auction economics
    /// live here so they can be tested without a running peer network.
    pub fn place_bid(&mut self, buyer: Address, amount: u64) -> Result<()> {
        if self.status == AuctionStatus::Closed || Utc::now().timestamp_millis() >= self.ends_at {
            return Err(anyhow!("Auction is closed"));
        }

        let current_price = self.highest_bid_amount();

        if amount <= current_price {
            return Err(anyhow!(
                "Bid amount must be greater than current price {}",
                current_price
            ));
        }

        self.bids.push(Bid {
            buyer,
            auction: self.key.clone(),
            amount,
        });

        Ok(())
    }

    pub fn close_if_expired(&mut self, now: i64) -> bool {
        if self.status == AuctionStatus::Closed || now < self.ends_at {
            return false;
        }

        self.status = AuctionStatus::Closed;
        if let Some(winning_bid) = self.bids.iter().max_by_key(|bid| bid.amount) {
            self.winner = Some(winning_bid.buyer.clone());
            self.winning_bid = Some(winning_bid.amount);
        }

        true
    }
}
impl TryFrom<&kademlia::Auction> for Auction {
    type Error = anyhow::Error;

    fn try_from(proto: &kademlia::Auction) -> Result<Self> {
        let key =
            Address::from_str(&proto.key).map_err(|e| anyhow!("Invalid key format: {}", e))?;

        let seller = Address::from_str(&proto.seller)
            .map_err(|e| anyhow!("Invalid seller address: {}", e))?;

        let bids = proto
            .bids
            .iter()
            .map(|b| {
                let buyer = Address::from_str(&b.buyer)
                    .map_err(|e| anyhow!("Invalid buyer address in bid: {}", e))?;
                let auction = Address::from_str(&b.auction)
                    .map_err(|e| anyhow!("Invalid auction key in bid: {}", e))?;
                Ok(Bid {
                    buyer,
                    auction,
                    amount: b.amount,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Auction {
            key,
            object: proto.object.clone(),
            initial_value: proto.initial_value,
            seller,
            bids,
            status: AuctionStatus::from_str(&proto.status)?,
            created_at: proto.created_at,
            ends_at: proto.ends_at,
            winner: if proto.winner.is_empty() {
                None
            } else {
                Some(
                    Address::from_str(&proto.winner)
                        .map_err(|e| anyhow!("Invalid winner address: {}", e))?,
                )
            },
            winning_bid: if proto.winning_bid == 0 {
                None
            } else {
                Some(proto.winning_bid)
            },
        })
    }
}

impl From<Auction> for kademlia::Auction {
    fn from(auction: Auction) -> Self {
        kademlia::Auction {
            key: auction.key.to_string(),
            object: auction.object,
            initial_value: auction.initial_value,
            seller: auction.seller.to_string(),
            bids: auction
                .bids
                .into_iter()
                .map(|bid| kademlia::Bid {
                    buyer: bid.buyer.to_string(),
                    auction: bid.auction.to_string(),
                    amount: bid.amount,
                })
                .collect(),
            status: auction.status.as_str().to_string(),
            created_at: auction.created_at,
            ends_at: auction.ends_at,
            winner: auction
                .winner
                .map(|winner| winner.to_string())
                .unwrap_or_default(),
            winning_bid: auction.winning_bid.unwrap_or(0),
        }
    }
}

impl From<&Auction> for kademlia::Auction {
    fn from(auction: &Auction) -> Self {
        kademlia::Auction {
            key: auction.key.to_string().clone(),
            object: auction.object.clone(),
            initial_value: auction.initial_value,
            seller: auction.seller.to_string(),
            bids: auction
                .bids
                .iter()
                .map(|bid| kademlia::Bid {
                    buyer: bid.buyer.to_string(),
                    auction: bid.auction.to_string(),
                    amount: bid.amount,
                })
                .collect(),
            status: auction.status.as_str().to_string(),
            created_at: auction.created_at,
            ends_at: auction.ends_at,
            winner: auction
                .winner
                .as_ref()
                .map(|winner| winner.to_string())
                .unwrap_or_default(),
            winning_bid: auction.winning_bid.unwrap_or(0),
        }
    }
}

use std::hash::{Hash, Hasher};

impl PartialEq for Auction {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for Auction {}

impl Hash for Auction {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bid {
    pub buyer: Address,
    pub auction: Address,
    pub amount: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auction_key_is_stable_for_same_offer() {
        let seller =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();

        let first = Auction::new("vintage keyboard".to_string(), 50, &seller);
        let second = Auction::new("vintage keyboard".to_string(), 50, &seller);

        assert_eq!(first.key, second.key);
    }

    #[test]
    fn auction_round_trips_through_protobuf() {
        let seller =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let buyer =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let mut auction = Auction::new("signed poster".to_string(), 25, &seller);
        auction.bids.push(Bid {
            buyer,
            auction: auction.key.clone(),
            amount: 30,
        });

        let proto: kademlia::Auction = (&auction).into();
        let decoded = Auction::try_from(&proto).unwrap();

        assert_eq!(auction.key, decoded.key);
        assert_eq!(auction.object, decoded.object);
        assert_eq!(auction.initial_value, decoded.initial_value);
        assert_eq!(auction.bids.len(), decoded.bids.len());
        assert_eq!(auction.bids[0].amount, decoded.bids[0].amount);
    }

    #[test]
    fn bid_must_be_higher_than_current_price() {
        let seller =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let buyer =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let mut auction = Auction::new("signed poster".to_string(), 25, &seller);

        assert!(auction.place_bid(buyer.clone(), 25).is_err());
        auction.place_bid(buyer.clone(), 30).unwrap();
        assert!(auction.place_bid(buyer, 30).is_err());
        assert_eq!(auction.highest_bid_amount(), 30);
    }

    #[test]
    fn closing_expired_auction_records_winner() {
        let seller =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let buyer =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let mut auction =
            Auction::new_with_duration("signed poster".to_string(), 25, &seller, 10_000);
        auction.place_bid(buyer.clone(), 30).unwrap();

        assert!(auction.close_if_expired(auction.ends_at));
        assert_eq!(auction.status, AuctionStatus::Closed);
        assert_eq!(auction.winner, Some(buyer));
        assert_eq!(auction.winning_bid, Some(30));
    }
}
