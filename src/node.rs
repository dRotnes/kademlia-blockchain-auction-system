use std::{
    collections::HashSet,
    fmt,
    str::FromStr,
    sync::{Arc, RwLock},
};

use crate::{
    auction::Auction, blockchain::address::Address, g_rpc::kademlia::NodeInformation,
    routing::routing_table::RoutingTable, utils::Config,
};
use anyhow::{anyhow, Result};

type SyncedAuctions = Arc<RwLock<HashSet<Auction>>>;
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
}

impl Node {
    pub fn new(id: Address, ip: String, port: u32, config: Config) -> Node {
        let node_info = NodeInfo { id, ip, port };
        let routing_table = RoutingTable::new(node_info.clone(), config.clone());

        Node {
            node_info,
            routing_table,
            config,
            auctions: Arc::new(RwLock::new(HashSet::new())),
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

        if auctions_map.contains(&auction) {
            return Ok(false);
        }

        info!("Stored auction with key: {}", auction.key);
        auctions_map.insert(auction);

        info!("{:?}", auctions_map);

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
            peer_sync_ms: 10,
            challenge_difficulty: 4,
            n_max_retries: 1,
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
        assert!(!node.store_auction(auction.clone()).unwrap());

        let found = node.find_auction(&auction.key).unwrap().unwrap();
        assert_eq!(found.key, auction.key);
        assert_eq!(found.object, "mechanical watch");
    }
}
