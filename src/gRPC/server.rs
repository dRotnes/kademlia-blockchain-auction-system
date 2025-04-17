use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Duration, Utc};
use ethereum_types::U256;
use tonic::{transport::Server, Request, Response, Status};
use anyhow::Result;
use tokio::sync::RwLock;
use crate::node::Node;
use crate::utils::{
    context::Context,
    execution::Runnable,
    generate_challenge,
    hash_data
};

use super::kademlia::kademlia_server::{Kademlia, KademliaServer};
use super::kademlia::{
    PingRequest,
    PingResponse,
    StoreRequest,
    StoreResponse,
    FindNodeRequest,
    FindNodeResponse,
    FindValueRequest,
    FindValueResponse,
    FoundValue,
    ClosestNodes,
    BootstrapRequest,
    BootstrapResponse,
    ChallengeResolutionRequest,
    ChallengeResolutionResponse
};


#[derive(Debug, Clone)]
pub struct SKademliaServer {
   node: Node,
   challenges_map: Arc<RwLock<HashMap<String, (U256, u32, i64)>>>,
}

impl SKademliaServer {
    pub fn new(context: &Context) -> SKademliaServer {
        SKademliaServer {
            node: context.node.clone(),
            challenges_map: Arc::new(RwLock::new(HashMap::new()))
        }
    }
}

impl Runnable for SKademliaServer {
    fn run(&self) -> Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(start_server(self.clone(), self.node.node_info.ip.clone(), self.node.config.port))?;
        Ok(())
    }
}

#[tonic::async_trait]
impl Kademlia for SKademliaServer {
    async fn ping(
        &self,
        request: Request<PingRequest>,
    ) -> Result<Response<PingResponse>, Status> {
        let sender = request.into_inner().sender.unwrap_or_default();
        info!("Received ping from: {:?}", sender);

        let reply = PingResponse {
            message: format!("Pong from server to {}", sender.ip),
        };
        Ok(Response::new(reply))
    }

    async fn bootstrap(
        &self,
        request: Request<BootstrapRequest>
    ) -> Result<Response<BootstrapResponse>, Status> {
        let sender = request.into_inner().sender.unwrap_or_default();
        info!("Received bootstrap request from: {:?}", sender);
        let difficulty = self.node.config.challenge_difficulty;
        let challenge_hash = generate_challenge(difficulty);

        // Save challenge for when we receive a response.
        let mut challenges_map_mut = self.challenges_map.write().await;
        let expiration = (Utc::now() + Duration::minutes(10)).timestamp_millis();
        challenges_map_mut.insert(sender.id.clone(), (challenge_hash.clone(), difficulty, expiration));

        let reply = BootstrapResponse {
            hash: challenge_hash.to_string(),
            difficulty
        };

        Ok(Response::new(reply))
    }

    async fn challenge_resolution(
        &self,
        request: Request<ChallengeResolutionRequest>
    ) -> Result<Response<ChallengeResolutionResponse>, Status> {
        let parsed_request = request.into_inner();
        let sender = parsed_request.sender.unwrap_or_default();
        let nonce = parsed_request.nonce;

        info!("Received challenge resolution from: {:?}", sender);

        // Lock the challenges_sent RwLock to safely access the challenges_sent HashMap.
        let challenge_opt = {
            let challenges_map = self.challenges_map.read().await;
            challenges_map.get(&sender.id).cloned()
        };    

        // Retrieve the challenge information for the given sender id.
        let mut accepted = false;
        if let Some(challenge) = challenge_opt {
            // Now you can verify the challenge by using the `hash` function.
            let expiration = challenge.2;
            if expiration < Utc::now().timestamp_millis() {
                accepted = false;
            }
            else {
                let challenge_hash = challenge.0;
                let challenge_difficulty = challenge.1;
                let data_to_hash = format!("{}{}", challenge_hash.to_string(), nonce);
                let hashed_data = hash_data(&data_to_hash);

                accepted = hashed_data.leading_zeros() >= challenge_difficulty;
            }

            // Remove challenge from challenges sent map.
            let mut challenges_sent_mut: tokio::sync::RwLockWriteGuard<'_, HashMap<String, (U256, u32, i64)>> = self.challenges_map.write().await;
            challenges_sent_mut.remove(&sender.id);
        }
        
        let reply = ChallengeResolutionResponse { 
            accepted
        };
        
        Ok(Response::new(reply))
    }

    async fn store(
        &self,
        request: Request<StoreRequest>,
    ) -> Result<Response<StoreResponse>, Status> {
        let req = request.into_inner();
        info!(
            "Store request from {}:{} | key: {}, value_len: {}",
            req.sender.as_ref().map(|s| &s.ip).unwrap_or(&"?".into()),
            req.sender.as_ref().map(|s| s.port).unwrap_or(0),
            req.key,
            req.value.len()
        );

        let reply = StoreResponse {
            message: format!("Stored key {}", req.key),
        };
        Ok(Response::new(reply))
    }

    async fn find_node(
        &self,
        request: Request<FindNodeRequest>,
    ) -> Result<Response<FindNodeResponse>, Status> {
        let req = request.into_inner();
        info!(
            "FindNode from {}:{} | target_id: {}",
            req.sender.as_ref().map(|s| &s.ip).unwrap_or(&"?".into()),
            req.sender.as_ref().map(|s| s.port).unwrap_or(0),
            req.target_id
        );

        let reply = FindNodeResponse {
            closest_nodes: vec![], // Replace with actual K-bucket lookups
        };
        Ok(Response::new(reply))
    }

    async fn find_value(
        &self,
        request: Request<FindValueRequest>,
    ) -> Result<Response<FindValueResponse>, Status> {
        let req = request.into_inner();
        info!(
            "FindValue from {}:{} | key: {}",
            req.sender.as_ref().map(|s| &s.ip).unwrap_or(&"?".into()),
            req.sender.as_ref().map(|s| s.port).unwrap_or(0),
            req.key
        );

        // Dummy logic: if key == "found", return a value. Otherwise, return closest nodes
        let result = if req.key == "found" {
            Some(FindValueResponse {
                result: Some(crate::gRPC::kademlia::find_value_response::Result::Value(FoundValue {
                    value: b"hello world".to_vec(),
                })),
            })
        } else {
            Some(FindValueResponse {
                result: Some(crate::gRPC::kademlia::find_value_response::Result::Nodes(ClosestNodes {
                    nodes: vec![], // Add real neighbors
                })),
            })
        };

        Ok(Response::new(result.unwrap()))
    }
}

async fn start_server(skademlia: SKademliaServer, ip: String, port:u32) -> Result<()> {
    let addr = format!("{}:{}", ip, port).parse()?;

    info!("Server listening on {}", addr);

    Server::builder()
        .add_service(KademliaServer::new(skademlia))
        .serve(addr)
        .await?;

    Ok(())
}