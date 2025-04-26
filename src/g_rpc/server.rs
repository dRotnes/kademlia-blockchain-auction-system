use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Duration, Utc};
use ethereum_types::U256;
use tonic::{transport::Server, Request, Response, Status};
use anyhow::{anyhow, Result};
use tokio::sync::RwLock;
use crate::g_rpc::kademlia::NodeInformation;
use crate::node::{Node, NodeInfo};
use crate::utils::{
    context::Context,
    execution::Runnable,
    crypto_own::{hash_data, sign_and_wrap, extract_and_verify},
    generate_challenge,
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
    ChallengeResolutionResponse,
    AuthenticatedMessage
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
        request: Request<AuthenticatedMessage>,
    ) -> Result<Response<AuthenticatedMessage>, Status> {
        let (_, parsed_request) = extract_and_verify::<PingRequest>(request.into_inner()).await?;
        let sender_proto = parsed_request
            .sender
            .ok_or_else(|| Status::invalid_argument("Missing sender information in request"))?;
        
        info!("Received ping from: {:?}", sender_proto);

        let reply = PingResponse {
            message: String::from("Pong"),
        };

        let auth_msg = sign_and_wrap(self.node.node_info.clone(), &reply, self.node.config.private_key.clone(), self.node.config.public_key.clone()).map_err(|e| Status::internal(format!("sign_and_wrap failed: {}", e)))?;

        Ok(Response::new(auth_msg))
    }

    async fn bootstrap(
        &self,
        request: Request<AuthenticatedMessage>
    ) -> Result<Response<AuthenticatedMessage>, Status> {
        let (_, parsed_request) = extract_and_verify::<BootstrapRequest>(request.into_inner()).await?;
        let sender_proto = parsed_request
            .sender
            .ok_or_else(|| Status::invalid_argument("Missing sender information in request"))?;
        
        info!("Received bootstrap request from: {:?}", sender_proto);
        let difficulty = self.node.config.challenge_difficulty;
        let challenge_hash = generate_challenge();

        // Save challenge for when we receive a response.
        let mut challenges_map_mut = self.challenges_map.write().await;
        let expiration = (Utc::now() + Duration::minutes(10)).timestamp_millis();
        challenges_map_mut.insert(sender_proto.id.clone(), (challenge_hash.clone(), difficulty, expiration));

        let reply = BootstrapResponse {
            hash: challenge_hash.to_string(),
            difficulty
        };

        let auth_msg = sign_and_wrap(self.node.node_info.clone(), &reply, self.node.config.private_key.clone(), self.node.config.public_key.clone()).map_err(|e| Status::internal(format!("sign_and_wrap failed: {}", e)))?;

        Ok(Response::new(auth_msg))
    }

    async fn challenge_resolution(
        &self,
        request: Request<AuthenticatedMessage>
    ) -> Result<Response<AuthenticatedMessage>, Status> {
        let (payload, parsed_request) = extract_and_verify::<ChallengeResolutionRequest>(request.into_inner()).await?;
        let sender_proto = parsed_request
            .sender
            .ok_or_else(|| Status::invalid_argument("Missing sender information in request"))?;

        let nonce = payload.nonce;

        info!("Received challenge resolution from: {:?}", sender_proto);

        // Lock the challenges_sent RwLock to safely access the challenges_sent HashMap.
        let challenge_opt = {
            let challenges_map = self.challenges_map.read().await;
            challenges_map.get(&sender_proto.id).cloned()
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
            challenges_sent_mut.remove(&sender_proto.id);
        }

        let reply = ChallengeResolutionResponse { 
            accepted,
        };
        
        let auth_msg = sign_and_wrap(self.node.node_info.clone(), &reply, self.node.config.private_key.clone(), self.node.config.public_key.clone()).map_err(|e| Status::internal(format!("sign_and_wrap failed: {}", e)))?;

        Ok(Response::new(auth_msg))
    }

    async fn store(
        &self,
        request: Request<AuthenticatedMessage>,
    ) -> Result<Response<AuthenticatedMessage>, Status> {
        let (payload, parsed_request) = extract_and_verify::<StoreRequest>(request.into_inner()).await?;

        info!(
            "Store request from {}:{} | key: {}, value_len: {}",
            parsed_request.sender.as_ref().map(|s| &s.ip).unwrap_or(&"?".into()),
            parsed_request.sender.as_ref().map(|s| s.port).unwrap_or(0),
            payload.key,
            payload.value.len()
        );

        let reply = StoreResponse {
            message: format!("Stored key {}", payload.key),
        };

        let auth_msg = sign_and_wrap(self.node.node_info.clone(), &reply, self.node.config.private_key.clone(), self.node.config.public_key.clone()).map_err(|e| Status::internal(format!("sign_and_wrap failed: {}", e)))?;

        Ok(Response::new(auth_msg))
    }

    async fn find_node(
        &self,
        request: Request<AuthenticatedMessage>,
    ) -> Result<Response<AuthenticatedMessage>, Status> {
        let (payload, parsed_request) = extract_and_verify::<FindNodeRequest>(request.into_inner()).await?;

        let sender_proto = parsed_request
            .sender
            .ok_or_else(|| Status::invalid_argument("Missing sender information in request"))?;

        let sender = NodeInfo::try_from(&sender_proto)
            .map_err(|e| Status::internal(format!("Failed to parse NodeInfo from sender_proto: {:?}, error: {}", sender_proto, e)))?;

        info!(
            "FindNode from {:?} | target_id: {}",
            &sender,
            payload.target_id
        );

        let node_id = U256::from_str_radix(&payload.target_id, 16)
            .map_err(|e| Status::internal(format!("Failed to parse target_id '{}': {}", payload.target_id, e)))?;

        let nodes_vector = self.node.find_node_in_routing_table(&node_id)
            .map(|node| vec![node])
            .unwrap_or_else(|| {
                let closest_nodes = self.node.get_closest_nodes_to_key(&node_id);
                if closest_nodes.is_empty() {
                    vec![NodeInformation::from(&self.node.node_info)]
                } else {
                    closest_nodes
                }
            });

        let reply = FindNodeResponse {
            closest_nodes: nodes_vector
        };

        // Add node to a kbucket if possible.
        self.node.insert_node_to_routing_table(sender);

        let auth_msg = sign_and_wrap(self.node.node_info.clone(), &reply, self.node.config.private_key.clone(), self.node.config.public_key.clone()).map_err(|e| Status::internal(format!("sign_and_wrap failed: {}", e)))?;

        Ok(Response::new(auth_msg))
    }

    async fn find_value(
        &self,
        request: Request<AuthenticatedMessage>,
    ) -> Result<Response<AuthenticatedMessage>, Status> {
        let (payload, parsed_request) = extract_and_verify::<FindValueRequest>(request.into_inner()).await?;

        info!(
            "FindValue from {}:{} | key: {}",
            parsed_request.sender.as_ref().map(|s| &s.ip).unwrap_or(&"?".into()),
            parsed_request.sender.as_ref().map(|s| s.port).unwrap_or(0),
            payload.key
        );

        // Dummy logic: if key == "found", return a value. Otherwise, return closest nodes
        let result = if payload.key == "found" {
            Some(FindValueResponse {
                result: Some(crate::g_rpc::kademlia::find_value_response::Result::Value(FoundValue {
                    value: b"hello world".to_vec(),
                })),
            })
        } else {
            Some(FindValueResponse {
                result: Some(crate::g_rpc::kademlia::find_value_response::Result::Nodes(ClosestNodes {
                    nodes: vec![],
                })),
            })
        };

        let reply = String::from("hey");

        let auth_msg = sign_and_wrap(self.node.node_info.clone(), &reply, self.node.config.private_key.clone(), self.node.config.public_key.clone()).map_err(|e| Status::internal(format!("sign_and_wrap failed: {}", e)))?;

        Ok(Response::new(auth_msg))
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