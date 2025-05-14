use std::sync::{Arc, RwLock};

use ethereum_types::U256;

use crate::{blockchain::address::Address, node::NodeInfo, utils::{calculate_distance, Config}};

use super::kbucket::KBucket;

type SyncedTable = Arc<RwLock<Vec<KBucket>>>; 

#[derive(Debug, Clone)]
pub struct RoutingTable {
    id: Address,
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
     *  Returns the index (0..254) of the highest differing bit.
     */
    pub fn get_bucket_index(&self, node_id: &Address) -> Option<usize> {
        let distance = self.id.distance(node_id);
        let lz = distance.leading_zeros() as usize;
    
        if lz == 256 {
            // distance == 0, same node
            return None;
        }
        if lz == 255 {
            return Some(0);
        }
    
        Some(254 - lz)
    }

    /**
     * Evict a node from the bucket.
     */
    pub fn evict_from_bucket(bucket: &mut KBucket, node_id: &Address) {
        
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

    pub fn insert_contact(&self, node_info: NodeInfo) -> Option<()> {
        if node_info.id == self.id {
            return None;
        }
    
        info!("Inserting new contact: {}", &node_info.id);
    
        let idx = self.get_bucket_index(&node_info.id)?;
    
        let Ok(mut table) = self.table.write() else {
            return None;
        };

        let bucket = &mut table[idx];
    
        // evict key from bucket if it exists
        Self::evict_from_bucket(bucket, &node_info.id);
    
        bucket.update_last_refresh_time();
    
        // If the list is full, drop the least recently seen node
        if bucket.len() == self.config.max_kbucket_entries {
            bucket.list.pop_front();
        }
        // Key is not in the list, try to insert
        bucket.list.push_back(node_info);
    
        Some(())
    }

    /**
     * Finds a node id in the table or returns None.
     */
    pub fn find_node_id(&self, node_id: &Address) -> Option<NodeInfo> {
        if node_id == &self.id {
            return None;
        }
    
        let idx = self.get_bucket_index(node_id)?;
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
    pub fn get_k_closest_nodes(&self, target_id: &Address, result: &mut Vec<NodeInfo>) {
        let table = self.table.read().unwrap();
        let mut visited = vec![false; table.len()];
        let maybe_target_idx = self.get_bucket_index(target_id);

        let mut bucket_indices = vec![];

        if let Some(target_idx) = maybe_target_idx {
            bucket_indices.push(target_idx);

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
        } else {
            // target == self.id (distance = 0)
            // So explore *all* buckets
            bucket_indices.extend(0..table.len());
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
        result.sort_by_key(|node| node.id.distance(&target_id));
        result.truncate(self.config.k_value);
    }
}
