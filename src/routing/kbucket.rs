use std::collections::LinkedList;

use crate::node::NodeInfo;

use chrono::Utc;

#[derive(Debug, Clone)]
pub struct KBucket {
    last_refresh_time: i64,
    pub list: LinkedList<NodeInfo>,
}

impl KBucket {
    pub fn new() -> Self {
        Self {
            last_refresh_time: Utc::now().timestamp_millis(),
            list: LinkedList::new(),
        }
    }

    /**
     * Updates the last_refresh_time.
     */
    pub fn update_last_refresh_time(&mut self) {
        self.last_refresh_time = Utc::now().timestamp_millis();
    }

    /**
     * Number of nodes in list.
     */
    pub fn len(&self) -> usize {
        self.list.len()
    }
}
