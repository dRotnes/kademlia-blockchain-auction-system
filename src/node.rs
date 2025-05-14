use std::{collections::{HashMap, HashSet}, fmt, str::FromStr, sync::{Arc, RwLock}};

use anyhow::{anyhow, Result};
use ethereum_types::U256;
use crate::{auction::Auction, blockchain::address::Address, g_rpc::kademlia::NodeInformation, routing::routing_table::RoutingTable, utils::Config};

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
            self.id,
            self.ip,
            self.port
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
        self.routing_table.get_k_closest_nodes(node_id, &mut closest_nodes);
            
        closest_nodes.into_iter()
            .map(|node| NodeInformation::from(&node))
            .collect()
    }

    pub fn store_auction(&self, auction: Auction) -> Result<bool> {
        let mut auctions_map = self.auctions.write()
            .map_err(|_| anyhow!("Failed to acquire write lock on auctions"))?;
        
        if auctions_map.contains(&auction) {
            return Ok(false);
        }    
        
        info!("Stored auction with key: {}", auction.key);
        auctions_map.insert(auction);

        info!("{:?}",auctions_map);
        
        Ok(true)
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