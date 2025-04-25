use anyhow::{anyhow, Result};
use ethereum_types::U256;
use crate::{g_rpc::kademlia::NodeInformation, routing::routing_table::RoutingTable, utils::{format_as_hex_string, Config}};

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: U256,
    pub ip: String,
    pub port: u32,
}
impl TryFrom<&NodeInformation> for NodeInfo {
    type Error = anyhow::Error;

    fn try_from(node: &NodeInformation) -> Result<Self> {
        let id = U256::from_str_radix(&node.id, 16)
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

    pub fn insert_node_to_routing_table(&self, node_info: NodeInfo) {
        self.routing_table.insert_contact(node_info);
    }

    pub fn find_node_in_routing_table(&self, node_id: &U256) -> Option<NodeInformation> {
        self.routing_table
            .find_node_id(node_id)
            .map(|node| NodeInformation::from(&node))
    }
    
    pub fn get_closest_nodes_to_key(&self, node_id: &U256) -> Vec<NodeInformation> {
        self.routing_table
            .get_k_closest_nodes(node_id)
            .into_iter()
            .map(|node| NodeInformation::from(&node))
            .collect()
    }
}

impl From<&NodeInfo> for NodeInformation {
    fn from(node: &NodeInfo) -> Self {
        NodeInformation {
            id: format_as_hex_string(node.id),
            ip: node.ip.clone(),
            port: node.port,
        }
    }
}