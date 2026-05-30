use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use crate::auction::Auction;
use crate::blockchain::address::Address;
use crate::g_rpc::kademlia::NodeInformation;
use crate::node::{Node, NodeInfo};
use crate::utils::{
    context,
    crypto_own::{extract_and_verify, hash_data, sign_and_wrap},
    execution::Runnable,
    generate_challenge, generate_url,
};
use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use ethereum_types::U256;
use tokio::sync::RwLock;
use tonic::{transport::Server, Request, Response, Status};

use super::kademlia::kademlia_client::KademliaClient;
use super::kademlia::kademlia_server::{Kademlia, KademliaServer};
use super::kademlia::{
    AuthenticatedMessage, BootstrapRequest, BootstrapResponse, ChallengeResolutionRequest,
    ChallengeResolutionResponse, ClosestNodes, FindNodeRequest, FindNodeResponse, FindValueRequest,
    FindValueResponse, FoundValue, PingRequest, PingResponse, StoreRequest, StoreResponse,
};

#[derive(Debug, Clone)]
pub struct SKademliaServer {
    node: Node,
    challenges_map: Arc<RwLock<HashMap<String, (U256, u32, i64)>>>,
}

impl SKademliaServer {
    pub fn new(context: &context::Context) -> SKademliaServer {
        SKademliaServer {
            node: context.node.clone(),
            challenges_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Runnable for SKademliaServer {
    fn run(&self) -> Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(start_server(
            self.clone(),
            self.node.node_info.ip.clone(),
            self.node.config.port,
        ))?;
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

        let sender = NodeInfo::try_from(&sender_proto).map_err(|e| {
            Status::internal(format!(
                "Failed to parse NodeInfo from sender_proto: {:?}, error: {}",
                sender_proto, e
            ))
        })?;

        info!("Received ping from: {}", sender);

        let reply = PingResponse {
            message: String::from("Pong"),
        };

        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(),
            &reply,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        )
        .map_err(|e| Status::internal(format!("sign_and_wrap failed: {}", e)))?;

        Ok(Response::new(auth_msg))
    }

    async fn bootstrap(
        &self,
        request: Request<AuthenticatedMessage>,
    ) -> Result<Response<AuthenticatedMessage>, Status> {
        let (_, parsed_request) =
            extract_and_verify::<BootstrapRequest>(request.into_inner()).await?;

        let sender_proto = parsed_request
            .sender
            .ok_or_else(|| Status::invalid_argument("Missing sender information in request"))?;

        let sender = NodeInfo::try_from(&sender_proto).map_err(|e| {
            Status::internal(format!(
                "Failed to parse NodeInfo from sender_proto: {:?}, error: {}",
                sender_proto, e
            ))
        })?;

        info!("Received bootstrap request from: {}", sender);
        let difficulty = self.node.config.challenge_difficulty;
        let challenge_hash = generate_challenge();

        // Save challenge for when we receive a response.
        let mut challenges_map_mut = self.challenges_map.write().await;
        let expiration = (Utc::now() + Duration::minutes(10)).timestamp_millis();
        challenges_map_mut.insert(
            sender_proto.id.clone(),
            (challenge_hash.clone(), difficulty, expiration),
        );

        let reply = BootstrapResponse {
            hash: challenge_hash.to_string(),
            difficulty,
        };

        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(),
            &reply,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        )
        .map_err(|e| Status::internal(format!("sign_and_wrap failed: {}", e)))?;

        Ok(Response::new(auth_msg))
    }

    async fn challenge_resolution(
        &self,
        request: Request<AuthenticatedMessage>,
    ) -> Result<Response<AuthenticatedMessage>, Status> {
        let (payload, parsed_request) =
            extract_and_verify::<ChallengeResolutionRequest>(request.into_inner()).await?;

        let sender_proto = parsed_request
            .sender
            .ok_or_else(|| Status::invalid_argument("Missing sender information in request"))?;

        let sender = NodeInfo::try_from(&sender_proto).map_err(|e| {
            Status::internal(format!(
                "Failed to parse NodeInfo from sender_proto: {:?}, error: {}",
                sender_proto, e
            ))
        })?;

        let nonce = payload.nonce;

        info!("Received challenge resolution from: {}", sender);

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
            } else {
                let challenge_hash = challenge.0;
                let challenge_difficulty = challenge.1;
                let data_to_hash = format!("{}{}", challenge_hash.to_string(), nonce);
                let hashed_data = hash_data(&data_to_hash);

                accepted = hashed_data.leading_zeros() >= challenge_difficulty;
            }

            // Remove challenge from challenges sent map.
            let mut challenges_sent_mut: tokio::sync::RwLockWriteGuard<
                '_,
                HashMap<String, (U256, u32, i64)>,
            > = self.challenges_map.write().await;
            challenges_sent_mut.remove(&sender_proto.id);
        }

        let reply = ChallengeResolutionResponse { accepted };

        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(),
            &reply,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        )
        .map_err(|e| Status::internal(format!("sign_and_wrap failed: {}", e)))?;

        Ok(Response::new(auth_msg))
    }

    async fn store(
        &self,
        request: Request<AuthenticatedMessage>,
    ) -> Result<Response<AuthenticatedMessage>, Status> {
        let (payload, parsed_request) =
            extract_and_verify::<StoreRequest>(request.into_inner()).await?;
        let sender_proto = parsed_request
            .sender
            .ok_or_else(|| Status::invalid_argument("Missing sender information in request"))?;

        let sender = NodeInfo::try_from(&sender_proto).map_err(|e| {
            Status::internal(format!(
                "Failed to parse NodeInfo from sender_proto: {:?}, error: {}",
                sender_proto, e
            ))
        })?;

        if payload.auction.is_none() {
            return Err(Status::invalid_argument(
                "Missing auction information in request",
            ));
        }

        info!("Received STORE from {}", sender);

        let auction: Auction = Auction::try_from(&payload.auction.unwrap())
            .map_err(|e| Status::internal(format!("Failed to deserialize Auction: {}", e)))?;

        let k_closest = self.node.get_closest_nodes_to_key(&auction.key);

        let mut message = format!("Forwarded auction with key: {}", auction.key);

        // If this node is among the closest, store it.
        let mut should_forward = true;
        if k_closest
            .iter()
            .any(|n| n.id == self.node.node_info.id.to_string())
        {
            should_forward = match self.node.store_auction(auction.clone()) {
                Ok(value) => value,
                Err(_) => false,
            };
            message = format!("Stored auction with key {}", auction.key);
        }

        // Forward store to other nodes.
        if should_forward {
            for node_info in k_closest {
                if node_info.id != self.node.node_info.id.to_string()
                    && node_info.id != sender.id.to_string()
                {
                    let _ = self
                        .send_store_to_node(node_info.ip.clone(), node_info.port, &auction)
                        .await;
                }
            }
        }

        let reply = StoreResponse { message };

        let response = sign_and_wrap(
            self.node.node_info.clone(),
            &reply,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        )
        .map_err(|e| Status::internal(format!("sign_and_wrap failed: {}", e)))?;

        Ok(Response::new(response))
    }

    async fn find_node(
        &self,
        request: Request<AuthenticatedMessage>,
    ) -> Result<Response<AuthenticatedMessage>, Status> {
        let (payload, parsed_request) =
            extract_and_verify::<FindNodeRequest>(request.into_inner()).await?;

        let sender_proto = parsed_request
            .sender
            .ok_or_else(|| Status::invalid_argument("Missing sender information in request"))?;

        let sender = NodeInfo::try_from(&sender_proto).map_err(|e| {
            Status::internal(format!(
                "Failed to parse NodeInfo from sender_proto: {:?}, error: {}",
                sender_proto, e
            ))
        })?;

        info!(
            "FindNode from {} | target_id: {}",
            &sender, payload.target_id
        );

        let node_id = Address::from_str(&payload.target_id).map_err(|e| {
            Status::internal(format!(
                "Failed to parse target_id '{}': {}",
                payload.target_id, e
            ))
        })?;

        let nodes_vector = self
            .node
            .find_node_in_routing_table(&node_id)
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
            closest_nodes: nodes_vector,
        };

        // Add node to a kbucket if possible.
        self.node.insert_node_to_routing_table(sender);

        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(),
            &reply,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        )
        .map_err(|e| Status::internal(format!("sign_and_wrap failed: {}", e)))?;

        Ok(Response::new(auth_msg))
    }

    async fn find_value(
        &self,
        request: Request<AuthenticatedMessage>,
    ) -> Result<Response<AuthenticatedMessage>, Status> {
        let (payload, parsed_request) =
            extract_and_verify::<FindValueRequest>(request.into_inner()).await?;

        let sender_proto = parsed_request
            .sender
            .ok_or_else(|| Status::invalid_argument("Missing sender information in request"))?;

        let sender = NodeInfo::try_from(&sender_proto).map_err(|e| {
            Status::internal(format!(
                "Failed to parse NodeInfo from sender_proto: {:?}, error: {}",
                sender_proto, e
            ))
        })?;

        info!("FindValue from {} | key: {}", sender, payload.key);

        let key = Address::from_str(&payload.key).map_err(|e| {
            Status::invalid_argument(format!("Invalid auction key '{}': {}", payload.key, e))
        })?;

        let response = if let Some(auction) = self
            .node
            .find_auction(&key)
            .map_err(|e| Status::internal(format!("Failed to read auction store: {}", e)))?
        {
            let mut value = Vec::new();
            let auction_proto: crate::g_rpc::kademlia::Auction = auction.into();
            prost::Message::encode(&auction_proto, &mut value)
                .map_err(|e| Status::internal(format!("Failed to encode auction value: {}", e)))?;

            FindValueResponse {
                result: Some(crate::g_rpc::kademlia::find_value_response::Result::Value(
                    FoundValue { value },
                )),
            }
        } else {
            FindValueResponse {
                result: Some(crate::g_rpc::kademlia::find_value_response::Result::Nodes(
                    ClosestNodes {
                        nodes: self.node.get_closest_nodes_to_key(&key),
                    },
                )),
            }
        };

        self.node.insert_node_to_routing_table(sender);

        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(),
            &response,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        )
        .map_err(|e| Status::internal(format!("sign_and_wrap failed: {}", e)))?;

        Ok(Response::new(auth_msg))
    }
}

impl SKademliaServer {
    async fn send_store_to_node(
        &self,
        node_ip: String,
        node_port: u32,
        auction: &Auction,
    ) -> Result<()> {
        let target = generate_url(&node_ip, node_port);
        let mut client = KademliaClient::connect(target.clone())
            .await
            .with_context(|| format!("Failed to connect to {} for ping", target))?;

        let store_request = StoreRequest {
            auction: Some(auction.clone().into()),
        };

        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(),
            &store_request,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        )?;

        let request = Request::new(auth_msg);

        let response = client
            .store(request)
            .await
            .with_context(|| "Failed to send store request")?
            .into_inner();

        let (payload, _) = extract_and_verify::<StoreResponse>(response)
            .await
            .with_context(|| "Failed to verify store response")?;

        info!("Store response: {}", payload.message);

        drop(client);
        Ok(())
    }
}

async fn start_server(skademlia: SKademliaServer, ip: String, port: u32) -> Result<()> {
    let addr = format!("{}:{}", ip, port).parse()?;

    info!("Server listening on {}", addr);

    Server::builder()
        .add_service(KademliaServer::new(skademlia))
        .serve(addr)
        .await?;

    Ok(())
}
