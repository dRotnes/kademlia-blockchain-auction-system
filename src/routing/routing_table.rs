use std::sync::{Arc, RwLock};

use ethereum_types::U256;

use crate::{node::NodeInfo, utils::{calculate_distance, Config}};

use super::kbucket::KBucket;

type SyncedTable = Arc<RwLock<Vec<KBucket>>>; 

#[derive(Debug, Clone)]
pub struct RoutingTable {
    id: U256,
    pub config: Config,
    pub table: SyncedTable,
}

impl RoutingTable { 
    pub fn new(node_info: NodeInfo, config: Config) -> RoutingTable {
        
        let rt = RoutingTable {
            id: node_info.id,
            config,
            table: Arc::new(RwLock::new(vec![KBucket::new(); 255]))
        };

        rt
    }

    /**
     *  Returns the index (0..255) of the highest differing bit.
     */
    pub fn get_bucket_index(&self, node_id: U256) -> usize {
        
        let distance = calculate_distance(self.id.clone(), node_id);
        let lz = distance.leading_zeros() as usize;
    
        if lz == 256 { return 0; }
        lz - 1
    }

    /**
     * Evict a node from the bucket.
     */
    pub fn evict_from_bucket(bucket: &mut KBucket, node_id: &U256) {
        
        // check if node is in the bucket
        let mut idx = None::<usize>;
        for (i, node_info) in bucket.list.iter().enumerate() {
            if node_info.id == *node_id {
                idx = Some(i);
                break;
            }
        }

        // remove node if it exists in the list
        if let Some(idx) = idx {
            
            info!("Evicting existing entry");
            // remove contact from list
            let mut split_list = bucket.list.split_off(idx);
            split_list.pop_front();
            bucket.list.append(&mut split_list);
        }
    }

    /**
     * Attempts to insert a new contact into the routing table,
     */
    pub fn insert_contact(&self, node_info: NodeInfo) -> Option<()> {
        
        if node_info.id == self.id {
            return None;
        }
        
        info!("Inserting new contact: {}", &node_info.id);
        let idx = self.get_bucket_index(node_info.id);

        let mut table = self.table.write().unwrap();
        let bucket = &mut table[idx];

        // evict key from bucket if it exists
        Self::evict_from_bucket(bucket, &node_info.id);

        bucket.update_last_refresh_time();

        // if the list is full, drop the least recently seen node
        if bucket.list.len() == self.config.max_kbucket_entries {
            bucket.list.pop_front();
        }
        // key is not in the list, try to insert
        bucket.list.push_back(node_info);
        
        return Some(());
    }

    /**
     * Finds a node id in the table or returns None.
     */
    pub fn find_node_id(&self, node_id: &U256) -> Option<NodeInfo> {
        if node_id == &self.id {
            return None;
        }

        let idx = self.get_bucket_index(*node_id);
        let table = self.table.read().ok()?;

        let bucket = &table[idx];
        for node in &bucket.list {
            if &node.id == node_id {
                return Some(node.clone());
            }
        }

        None
    }

    /**
     * Returns up to `k_value` closest nodes to the given `target_id`.
     */
    pub fn get_k_closest_nodes(&self, target_id: &U256) -> Vec<NodeInfo> {
        let table = self.table.read().unwrap();
        let mut result: Vec<NodeInfo> = Vec::new();
        let mut visited = vec![false; table.len()];

        let target_idx = self.get_bucket_index(*target_id);
        let mut bucket_indices = vec![target_idx];

        // Add neighboring indices symmetrically
        for i in 1..table.len() {
            let lower = target_idx.checked_sub(i);
            let upper = target_idx + i;

            if let Some(l) = lower {
                if l < table.len() && !visited[l] {
                    bucket_indices.push(l);
                    visited[l] = true;
                }
            }

            if upper < table.len() && !visited[upper] {
                bucket_indices.push(upper);
                visited[upper] = true;
            }

            if bucket_indices.len() >= table.len() {
                break;
            }
        }

        // Collect all nodes from the selected buckets
        for &idx in &bucket_indices {
            let bucket = &table[idx];
            for node in &bucket.list {
                if node.id != self.id {
                    result.push(node.clone());
                }
            }

            if result.len() >= self.config.k_value {
                break;
            }
        }

        // Sort nodes by XOR distance to the target_id
        result.sort_by_key(|node| calculate_distance(node.id, *target_id));
        result.truncate(self.config.k_value);

        result
    }
}
