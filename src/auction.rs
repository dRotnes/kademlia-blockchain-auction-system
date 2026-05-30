use anyhow::{anyhow, Result};
use std::str::FromStr;

use crate::g_rpc::kademlia;
use crate::utils::format_as_hex_string;
use crate::{blockchain::address::Address, utils::crypto_own::hash_data};

#[derive(Debug, Clone)]
pub struct Auction {
    pub key: Address,
    pub object: String,
    pub initial_value: u64,
    pub seller: Address,
    pub bids: Vec<Bid>,
}
impl Auction {
    /// Create a new auction and derive its DHT key from immutable auction data.
    ///
    /// The key intentionally excludes bids so the auction can be found by the
    /// same address as bids are appended.
    pub fn new(object: String, initial_value: u64, seller: &Address) -> Auction {
        let mut auction = Auction {
            key: Address::default(),
            object,
            initial_value,
            seller: seller.clone(),
            bids: Vec::new(),
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
                Ok(Bid { buyer, auction })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Auction {
            key,
            object: proto.object.clone(),
            initial_value: proto.initial_value,
            seller,
            bids,
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
                })
                .collect(),
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
                })
                .collect(),
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

#[derive(Debug, Clone)]
pub struct Bid {
    pub buyer: Address,
    pub auction: Address,
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
        });

        let proto: kademlia::Auction = (&auction).into();
        let decoded = Auction::try_from(&proto).unwrap();

        assert_eq!(auction.key, decoded.key);
        assert_eq!(auction.object, decoded.object);
        assert_eq!(auction.initial_value, decoded.initial_value);
        assert_eq!(auction.bids.len(), decoded.bids.len());
    }
}
