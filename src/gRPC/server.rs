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
    crypto_own::verify_signature,
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
    ) -> Result<Response<PingResponse>, Status> {
        let verified_payload: PingRequest = extract_and_verify(request.into_inner()).await?;
        let sender = verified_payload.sender.unwrap_or_default();

        info!("Received ping from: {:?}", sender);

        let reply = PingResponse {
            message: format!("Pong from server to {}", sender.ip),
        };
        Ok(Response::new(reply))
    }

    async fn bootstrap(
        &self,
        request: Request<AuthenticatedMessage>
    ) -> Result<Response<BootstrapResponse>, Status> {
        let verified_payload: BootstrapRequest = extract_and_verify(request.into_inner()).await?;
        let sender = verified_payload.sender.unwrap_or_default();
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
        request: Request<AuthenticatedMessage>
    ) -> Result<Response<ChallengeResolutionResponse>, Status> {
        let verified_payload: ChallengeResolutionRequest = extract_and_verify(request.into_inner()).await?;
        let sender = verified_payload.sender.unwrap_or_default();
        let nonce = verified_payload.nonce;

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
        request: Request<AuthenticatedMessage>,
    ) -> Result<Response<StoreResponse>, Status> {
        let verified_payload: StoreRequest = extract_and_verify(request.into_inner()).await?;

        info!(
            "Store request from {}:{} | key: {}, value_len: {}",
            verified_payload.sender.as_ref().map(|s| &s.ip).unwrap_or(&"?".into()),
            verified_payload.sender.as_ref().map(|s| s.port).unwrap_or(0),
            verified_payload.key,
            verified_payload.value.len()
        );

        let reply = StoreResponse {
            message: format!("Stored key {}", verified_payload.key),
        };
        Ok(Response::new(reply))
    }

    async fn find_node(
        &self,
        request: Request<AuthenticatedMessage>,
    ) -> Result<Response<FindNodeResponse>, Status> {
        let verified_payload: FindNodeRequest = extract_and_verify(request.into_inner()).await?;

        info!(
            "FindNode from {}:{} | target_id: {}",
            verified_payload.sender.as_ref().map(|s| &s.ip).unwrap_or(&"?".into()),
            verified_payload.sender.as_ref().map(|s| s.port).unwrap_or(0),
            verified_payload.target_id
        );

        let reply = FindNodeResponse {
            closest_nodes: vec![],
        };
        Ok(Response::new(reply))
    }

    async fn find_value(
        &self,
        request: Request<AuthenticatedMessage>,
    ) -> Result<Response<FindValueResponse>, Status> {
        let verified_payload: FindValueRequest = extract_and_verify(request.into_inner()).await?;

        info!(
            "FindValue from {}:{} | key: {}",
            verified_payload.sender.as_ref().map(|s| &s.ip).unwrap_or(&"?".into()),
            verified_payload.sender.as_ref().map(|s| s.port).unwrap_or(0),
            verified_payload.key
        );

        // Dummy logic: if key == "found", return a value. Otherwise, return closest nodes
        let result = if verified_payload.key == "found" {
            Some(FindValueResponse {
                result: Some(crate::gRPC::kademlia::find_value_response::Result::Value(FoundValue {
                    value: b"hello world".to_vec(),
                })),
            })
        } else {
            Some(FindValueResponse {
                result: Some(crate::gRPC::kademlia::find_value_response::Result::Nodes(ClosestNodes {
                    nodes: vec![],
                })),
            })
        };

        Ok(Response::new(result.unwrap()))
    }
}

async fn extract_and_verify<T: prost::Message + Default>(
    msg: AuthenticatedMessage,
) -> Result<T, Status> {
    info!("{:?}", &msg);
    let is_valid = verify_signature(&msg.payload, &msg.signature, &msg.public_key)
        .map_err(|e| Status::unauthenticated(format!("Signature check error: {:?}", e)))?;

    if !is_valid {
        return Err(Status::unauthenticated("Invalid signature"));
    }

    let payload = T::decode(&*msg.payload)
        .map_err(|e| Status::invalid_argument(format!("Failed to decode message: {:?}", e)))?;

    Ok(payload)
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