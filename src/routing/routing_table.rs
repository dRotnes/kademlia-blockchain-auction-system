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
        let mut rt = RoutingTable {
            id: node_info.id,
            config,
            table: Arc::new(RwLock::new(vec![KBucket::new(); 256]))
        };

        let _ = rt.insert_contact(node_info);
        rt
    }

    /**
     *  Returns the index (0..255) of the highest differing bit.
     */
    pub fn get_bucket_index(&self, node_id: U256) -> usize {
        let distance = calculate_distance(self.id.clone(), node_id);
        let lz = distance.leading_zeros() as usize;
    
        if lz == 256 {
            0
        } else {
            255 - lz
        }
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
    pub fn insert_contact(&mut self, node_info: NodeInfo) -> Option<()> {
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
}