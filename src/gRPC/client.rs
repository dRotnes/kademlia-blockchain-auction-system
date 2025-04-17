use ethereum_types::U256;
use tonic::Request;
use anyhow::Result;
use std::pin::Pin;

use crate::gRPC::kademlia::{BootstrapRequest, ChallengeResolutionRequest};
use crate::node::Node;
use crate::utils::execution::sleep_millis;
use crate::utils::{
    context::Context,
    execution::Runnable,
    proof_of_work,
};
use super::kademlia::kademlia_client::KademliaClient;
use super::kademlia::{PingRequest, NodeInfo};

#[derive(Clone)]
pub struct SKademliaClient {
    node: Node
}

impl SKademliaClient {
    pub fn new(context: &Context) -> SKademliaClient {
        SKademliaClient { 
            node: context.node.clone()
         }
    }
}

#[tonic::async_trait]
impl Runnable for SKademliaClient {
    fn run(&self) -> Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            // This is the FIX: bootstrap only if this is NOT the bootstrap node.
            if self.node.node_info.port == self.node.config.bootstrap_peer_port {
                info!("This is the bootstrap node, skipping bootstrap...");
                self.start().await
            } else {
                self.try_bootstrap().await
            }
        })
    }
}

impl SKademliaClient {

    async fn start(&self) -> Result<()>{
        info!("Node running");
        loop {
            sleep_millis(self.node.config.peer_sync_ms);
        }

    }

    async fn try_bootstrap(&self) -> Result<()> {
        for attempt in 0..self.node.config.n_max_retries {
            info!("Attempt {} to bootstrap...", attempt + 1);

            let (challenge_hash_string, difficulty) = match self.bootstrap(
                self.node.config.bootstrap_peer_ip.clone(),
                self.node.config.bootstrap_peer_port
            ).await {
                Ok(result) => result,
                Err(e) => {
                    error!("Bootstrap request failed: {:?}", e);
                    continue;
                }
            };

            info!("Solving bootstrap challenge...");
            let challenge_solution = proof_of_work(&challenge_hash_string, difficulty);

            let response = self.challenge_resolution(
                self.node.config.bootstrap_peer_ip.clone(),
                self.node.config.bootstrap_peer_port,
                challenge_solution
            ).await;

            match response {
                Ok(true) => {
                    info!("Bootstrap challenge solved successfully.");
                    return self.start().await;
                }
                Ok(false) => {
                    warn!("Challenge rejected. Retrying...");
                }
                Err(e) => {
                    error!("Challenge resolution failed: {:?}", e);
                }
            }
        }

        error!("Exceeded max retries. Exiting.");
        std::process::exit(1);
    }

    async fn ping(&self, node_ip: String, node_port:u32) -> Result<()> {
        let target = format!("http://{}:{}", node_ip, node_port);
        let mut client = KademliaClient::connect(target).await?;

        let request = Request::new(PingRequest {
            sender: Some(NodeInfo {
                id: self.node.node_info.id.to_string(),
                ip: "127.0.0.1".to_string(),
                port: self.node.node_info.port,
            }),
        });

        let response = client.ping(request).await?.into_inner();

        info!("Ping response: {}", response.message);
        Ok(())
    }

    async fn bootstrap(&self, bootstrap_node_ip: String, bootstrap_node_port:u32) -> Result<(String, u32)> {
        let target = format!("http://{}:{}", bootstrap_node_ip, bootstrap_node_port);
        let mut client = KademliaClient::connect(target).await?;

        let request = Request::new(BootstrapRequest {
            sender: Some(NodeInfo {
                id: self.node.node_info.id.to_string(),
                ip: self.node.node_info.ip.clone(),
                port: self.node.node_info.port,
            }),
        });

        let response = client.bootstrap(request).await?.into_inner();

        info!("Challenge received from bootstrap node");
        Ok((response.hash, response.difficulty))
    }

    async fn challenge_resolution(&self, bootstrap_node_ip: String, bootstrap_node_port:u32, challenge_resolution: u64) -> Result<bool> {
        let target = format!("http://{}:{}", bootstrap_node_ip, bootstrap_node_port);
        let mut client = KademliaClient::connect(target).await?;

        let request = Request::new(ChallengeResolutionRequest {
            sender: Some(NodeInfo {
                id: self.node.node_info.id.to_string(),
                ip: self.node.node_info.ip.clone(),
                port: self.node.node_info.port,
            }),
            nonce: challenge_resolution
        });

        let response = client.challenge_resolution(request).await?.into_inner();

        info!("Received challenge resolution response from bootstrap node");
        Ok(response.accepted)
    }
}
