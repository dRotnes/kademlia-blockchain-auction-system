use std::{
    collections::HashSet,
    fmt,
    str::FromStr,
    sync::{Arc, RwLock},
};

use crate::{
    auction::Auction,
    blockchain::{address::Address, block::Block, transaction::Transaction},
    consensus,
    g_rpc::kademlia::NodeInformation,
    routing::routing_table::RoutingTable,
    storage,
    utils::Config,
};
use anyhow::{anyhow, Result};
use ethereum_types::U256;

type SyncedAuctions = Arc<RwLock<HashSet<Auction>>>;
type SyncedLedger = Arc<RwLock<Vec<Block>>>;
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: Address,
    pub ip: String,
    pub port: u32,
}
impl fmt::Display for NodeInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "NodeInfo {{ id: {}, address: {}:{} }}",
            self.id, self.ip, self.port
        )
    }
}
impl TryFrom<&NodeInformation> for NodeInfo {
    type Error = anyhow::Error;

    fn try_from(node: &NodeInformation) -> Result<Self> {
        let id = Address::from_str(&node.id)
            .map_err(|e| anyhow!("Invalid hex ID in response: {}", e))?;

        Ok(NodeInfo {
            id,
            ip: node.ip.clone(),
            port: node.port,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub node_info: NodeInfo,
    pub routing_table: RoutingTable,
    pub config: Config,
    pub auctions: SyncedAuctions,
    pub ledger: SyncedLedger,
}

impl Node {
    pub fn new(id: Address, ip: String, port: u32, config: Config) -> Node {
        let node_info = NodeInfo { id, ip, port };
        let routing_table = RoutingTable::new(node_info.clone(), config.clone());
        let auctions = storage::load_auctions(&config.data_dir)
            .unwrap_or_else(|error| {
                warn!("Failed to load persisted auctions: {error:#}");
                Vec::new()
            })
            .into_iter()
            .collect();
        let ledger = storage::load_ledger(&config.data_dir).unwrap_or_else(|error| {
            warn!("Failed to load persisted ledger: {error:#}");
            Vec::new()
        });

        Node {
            node_info,
            routing_table,
            config,
            auctions: Arc::new(RwLock::new(auctions)),
            ledger: Arc::new(RwLock::new(ledger)),
        }
    }

    pub fn insert_node_to_routing_table(&self, node_info: NodeInfo) {
        self.routing_table.insert_contact(node_info);
    }

    pub fn find_node_in_routing_table(&self, node_id: &Address) -> Option<NodeInformation> {
        self.routing_table
            .find_node_id(node_id)
            .map(|node| NodeInformation::from(&node))
    }

    pub fn get_closest_nodes_to_key(&self, node_id: &Address) -> Vec<NodeInformation> {
        let mut closest_nodes: Vec<NodeInfo> = vec![self.node_info.clone()];
        self.routing_table
            .get_k_closest_nodes(node_id, &mut closest_nodes);

        closest_nodes
            .into_iter()
            .map(|node| NodeInformation::from(&node))
            .collect()
    }

    pub fn store_auction(&self, auction: Auction) -> Result<bool> {
        let mut auctions_map = self
            .auctions
            .write()
            .map_err(|_| anyhow!("Failed to acquire write lock on auctions"))?;

        let action = if auctions_map.replace(auction.clone()).is_some() {
            "Updated"
        } else {
            "Stored"
        };

        info!("{} auction with key: {}", action, auction.key);
        self.persist_auctions_locked(&auctions_map)?;
        if let (Some(winner), Some(winning_bid)) = (auction.winner.clone(), auction.winning_bid) {
            self.record_auction_win(
                auction.key.clone(),
                auction.seller.clone(),
                winner,
                winning_bid,
            )?;
        }

        Ok(true)
    }

    /// Look up an auction held by this node.
    ///
    /// Kademlia `FIND_VALUE` first checks the local store. If the value is not
    /// present, the caller receives the closest known nodes and can continue the
    /// lookup iteratively.
    pub fn find_auction(&self, key: &Address) -> Result<Option<Auction>> {
        let auctions = self
            .auctions
            .read()
            .map_err(|_| anyhow!("Failed to acquire read lock on auctions"))?;

        Ok(auctions.iter().find(|auction| &auction.key == key).cloned())
    }

    pub fn all_auctions(&self) -> Result<Vec<Auction>> {
        let auctions = self
            .auctions
            .read()
            .map_err(|_| anyhow!("Failed to acquire read lock on auctions"))?;

        Ok(auctions.iter().cloned().collect())
    }

    pub fn close_expired_auctions(&self, now: i64) -> Result<Vec<Auction>> {
        let mut closed = Vec::new();
        let mut auctions = self
            .auctions
            .write()
            .map_err(|_| anyhow!("Failed to acquire write lock on auctions"))?;
        let mut updated = Vec::new();

        for mut auction in auctions.iter().cloned() {
            if auction.close_if_expired(now) {
                if let (Some(winner), Some(winning_bid)) =
                    (auction.winner.clone(), auction.winning_bid)
                {
                    self.record_auction_win(
                        auction.key.clone(),
                        auction.seller.clone(),
                        winner,
                        winning_bid,
                    )?;
                }
                closed.push(auction.clone());
            }
            updated.push(auction);
        }

        *auctions = updated.into_iter().collect();
        self.persist_auctions_locked(&auctions)?;

        Ok(closed)
    }

    fn record_auction_win(
        &self,
        auction: Address,
        seller: Address,
        winner: Address,
        winning_bid: u64,
    ) -> Result<()> {
        let mut ledger = self
            .ledger
            .write()
            .map_err(|_| anyhow!("Failed to acquire write lock on ledger"))?;

        if ledger.iter().any(|block| {
            block.transactions.iter().any(|transaction| {
                matches!(
                    &transaction.kind,
                    crate::blockchain::transaction::TransactionKind::AuctionWon { auction: won }
                        if won == &auction
                )
            })
        }) {
            return Ok(());
        }

        if !consensus::has_reputation_to_commit(&self.config, &ledger, &self.node_info.id) {
            warn!(
                "Skipping auction win block because node {} does not meet proof-of-reputation threshold",
                self.node_info.id
            );
            return Ok(());
        }

        let previous_hash = ledger
            .last()
            .map(|block| block.hash)
            .unwrap_or_else(U256::zero);
        let block = Block::new(
            ledger.len() as u64,
            0,
            previous_hash,
            vec![Transaction::auction_won(
                auction,
                seller,
                winner,
                winning_bid,
            )],
        );
        ledger.push(block);
        storage::save_ledger(&self.config.data_dir, &ledger)?;
        Ok(())
    }

    fn persist_auctions_locked(&self, auctions: &HashSet<Auction>) -> Result<()> {
        let values = auctions.iter().cloned().collect::<Vec<_>>();
        storage::save_auctions(&self.config.data_dir, &values)
    }
}

impl From<&NodeInfo> for NodeInformation {
    fn from(node: &NodeInfo) -> Self {
        NodeInformation {
            id: node.id.to_string(),
            ip: node.ip.clone(),
            port: node.port,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
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
            data_dir: format!(
                "./target/test/node-{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            ),
            peer_sync_ms: 10,
            challenge_difficulty: 4,
            n_max_retries: 1,
            consensus_mode: "pow".to_string(),
            reputation_threshold: 1,
            automation_enabled: false,
            automation_interval_ms: 1000,
            auction_duration_ms: 1000,
        }
    }

    #[test]
    fn node_can_store_and_find_auction_by_key() {
        let node_id =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let node = Node::new(
            node_id.clone(),
            "127.0.0.1".to_string(),
            8000,
            test_config(),
        );
        let auction = Auction::new("mechanical watch".to_string(), 100, &node_id);

        assert!(node.store_auction(auction.clone()).unwrap());
        assert!(node.store_auction(auction.clone()).unwrap());

        let found = node.find_auction(&auction.key).unwrap().unwrap();
        assert_eq!(found.key, auction.key);
        assert_eq!(found.object, "mechanical watch");
    }

    #[test]
    fn storing_existing_auction_replaces_older_bid_state() {
        let node_id =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let bidder =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let node = Node::new(
            node_id.clone(),
            "127.0.0.1".to_string(),
            8000,
            test_config(),
        );
        let mut auction = Auction::new("mechanical watch".to_string(), 100, &node_id);

        node.store_auction(auction.clone()).unwrap();
        auction.place_bid(bidder, 125).unwrap();
        node.store_auction(auction.clone()).unwrap();

        let found = node.find_auction(&auction.key).unwrap().unwrap();
        assert_eq!(found.bids.len(), 1);
        assert_eq!(found.highest_bid_amount(), 125);
    }

    #[test]
    fn closing_expired_auction_persists_a_winner_block() {
        let node_id =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let bidder =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let node = Node::new(
            node_id.clone(),
            "127.0.0.1".to_string(),
            8000,
            test_config(),
        );
        let mut auction =
            Auction::new_with_duration("mechanical watch".to_string(), 100, &node_id, 1);
        auction.place_bid(bidder.clone(), 125).unwrap();
        node.store_auction(auction.clone()).unwrap();

        let closed = node.close_expired_auctions(auction.ends_at).unwrap();

        assert_eq!(closed.len(), 1);
        let ledger = node.ledger.read().unwrap();
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger[0].transactions[0].sender, bidder);
    }
}
