use ethereum_types::U256;

use crate::{routing::routing_table::RoutingTable, utils::Config};

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: U256,
    pub ip: String,
    pub port: u32,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub node_info: NodeInfo,
    pub routing_table: RoutingTable,
    pub config: Config,
}

impl Node {
    pub fn new(id: U256, ip: String, port: u32, config: Config) -> Node {
        let node_info = NodeInfo { id, ip, port };
        let routing_table = RoutingTable::new(node_info.clone(), config.clone());

        Node {
            node_info,
            routing_table,
            config,
        }
    }

    pub fn insert_node_to_routing_table(&mut self, node_info: NodeInfo) {
        
    }
}