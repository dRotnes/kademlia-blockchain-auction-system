use ethereum_types::U256;
use tonic::Request;
use anyhow::{anyhow, Context, Result};

use crate::g_rpc::kademlia::{BootstrapRequest, BootstrapResponse, ChallengeResolutionRequest, ChallengeResolutionResponse, PingResponse};
use crate::node::{Node, NodeInfo};
use crate::utils::format_as_hex_string;
use crate::utils::{
    context,
    execution::{Runnable, sleep_millis},
    proof_of_work,
    crypto_own::{sign_and_wrap, extract_and_verify},
};
use super::kademlia::kademlia_client::KademliaClient;
use super::kademlia::{FindNodeRequest, PingRequest};

#[derive(Clone)]
pub struct SKademliaClient {
    node: Node
}

impl SKademliaClient {
    pub fn new(context: &context::Context) -> SKademliaClient {
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

    async fn start(&self) -> Result<()> {
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
                Ok((true, bootstrap_node_info)) => {
                    info!("Bootstrap challenge solved successfully.");
                    // Insert bootstrap node into routing table after succesfully solving challenge.
                    self.node.insert_node_to_routing_table(bootstrap_node_info);
                    let find_node = self.find_node(self.node.config.bootstrap_peer_ip.clone(), self.node.config.bootstrap_peer_port, self.node.node_info.id.clone()).await?;
                    // Start node.
                    return self.start().await;
                }
                Ok((false, _)) => {
                    warn!("Challenge rejected. Retrying...");
                }
                Err(e) => {
                    error!("Challenge resolution failed: {:?}", e);
                }
            }
            sleep_millis(self.node.config.peer_sync_ms);
        }

        error!("Exceeded max retries. Exiting.");
        std::process::exit(1);
    }

    async fn ping(&self, node_ip: String, node_port: u32) -> Result<()> {
        let target = generate_url(&node_ip, node_port);
    
        let mut client = KademliaClient::connect(target.clone())
            .await
            .with_context(|| format!("Failed to connect to {} for ping", target))?;
    
        let inner_request = PingRequest {};
    
        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(),
            &inner_request,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        )
        .with_context(|| "Failed to sign ping request")?;
    
        let request = Request::new(auth_msg);
    
        let response = client
            .ping(request)
            .await
            .with_context(|| "Failed to send ping request")?
            .into_inner();
    
        let (payload, _) = extract_and_verify::<PingResponse>(response)
            .await
            .with_context(|| "Failed to verify ping response")?;
    
        info!("Ping response: {}", payload.message);
        Ok(())
    }

    async fn bootstrap(&self, bootstrap_node_ip: String, bootstrap_node_port: u32) -> Result<(String, u32)> {
        let target = generate_url(&bootstrap_node_ip, bootstrap_node_port);
    
        let mut client = KademliaClient::connect(target.clone())
            .await
            .with_context(|| format!("Failed to connect to bootstrap node at {}", target))?;
    
        let inner_request = BootstrapRequest {};
    
        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(),
            &inner_request,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        )
        .with_context(|| "Failed to sign bootstrap request")?;
    
        let request = Request::new(auth_msg);
    
        let response = client
            .bootstrap(request)
            .await
            .with_context(|| "Failed to send bootstrap request")?
            .into_inner();
    
        let (payload, _) = extract_and_verify::<BootstrapResponse>(response)
            .await
            .with_context(|| "Failed to verify bootstrap response")?;
    
        info!("Challenge received from bootstrap node");
        Ok((payload.hash, payload.difficulty))
    }

    async fn challenge_resolution(&self, bootstrap_node_ip: String, bootstrap_node_port: u32, challenge_resolution: u64) -> Result<(bool, NodeInfo)> {
        let target = generate_url(&bootstrap_node_ip, bootstrap_node_port);
        let mut client = KademliaClient::connect(target.clone()).await.with_context(|| format!("Failed to connect to {}", target))?;

        let inner_request = ChallengeResolutionRequest {
            nonce: challenge_resolution,
        };

        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(), 
            &inner_request,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone()).with_context(|| "Failed to sign challenge resolution request")?;

        let request = Request::new(auth_msg);

        let response = client
            .challenge_resolution(request)
            .await
            .with_context(|| "Failed to send challenge resolution request")?
            .into_inner();

        let (payload, parsed_response) = extract_and_verify::<ChallengeResolutionResponse>(response)
            .await
            .with_context(|| "Failed to extract/verify challenge resolution response")?;

        let sender = parsed_response.sender.ok_or_else(|| anyhow!("Missing sender info in challenge response"))?;

        info!("Received challenge resolution response from bootstrap node");
        Ok((payload.accepted, NodeInfo::try_from(&sender)?))
    }

    async fn find_node(&self, node_ip: String, node_port: u32, target_id: U256) -> Result<Vec<NodeInfo>> {
        let target = generate_url(&node_ip, node_port);

        let mut client = KademliaClient::connect(target.clone())
            .await
            .with_context(|| format!("Failed to connect to {} for find_node", target))?;

        let inner_request = FindNodeRequest {
            target_id: format_as_hex_string(target_id),
        };

        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(),
            &inner_request,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        ).with_context(|| "Failed to sign find_node request")?;

        let request = Request::new(auth_msg);

        let response = client
            .find_node(request)
            .await
            .with_context(|| "Failed to send find_node request")?
            .into_inner();

        let (payload, _) = extract_and_verify::<crate::g_rpc::kademlia::FindNodeResponse>(response)
            .await
            .with_context(|| "Failed to verify find_node response")?;

        let closest_nodes = payload.closest_nodes.into_iter()
            .map(|node_info| {
                Ok(NodeInfo::try_from(&node_info)?)
            })
            .collect::<Result<Vec<NodeInfo>>>()?;

        info!("{:?}", closest_nodes);

        Ok(closest_nodes)
    }
}

/**
 * Generates an url based on node ip and port.
 */
fn generate_url(node_ip: &str, node_port: u32) -> String {
    format!("http://{}:{}", node_ip, node_port)
}