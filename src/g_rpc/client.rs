use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use prost::Message;
use rand::{seq::SliceRandom, Rng};
use tonic::Request;

use super::kademlia::kademlia_client::KademliaClient;
use super::kademlia::{FindNodeRequest, PingRequest, StoreRequest};
use crate::auction::{Auction, AuctionStatus};
use crate::blockchain::address::Address;
use crate::command::Command;
use crate::g_rpc::kademlia::{
    find_value_response, BootstrapRequest, BootstrapResponse, ChallengeResolutionRequest,
    ChallengeResolutionResponse, FindNodeResponse, FindValueResponse, PingResponse, StoreResponse,
};
use crate::node::{Node, NodeInfo};
use crate::utils::{
    context,
    crypto_own::{extract_and_verify, sign_and_wrap},
    execution::{sleep_millis, Runnable},
    generate_url, proof_of_work,
};
use std::collections::{HashSet, VecDeque};

#[derive(Clone)]
pub struct SKademliaClient {
    node: Node,
    command: Command,
}

impl SKademliaClient {
    pub fn new(context: &context::Context, command: Command) -> SKademliaClient {
        SKademliaClient {
            node: context.node.clone(),
            command,
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
            if self.node.config.automation_enabled {
                if let Err(error) = self.run_automation_tick().await {
                    warn!("Automation tick failed: {error:#}");
                }
                sleep_millis(self.node.config.automation_interval_ms);
            } else {
                sleep_millis(self.node.config.peer_sync_ms);
            }
        }
    }

    async fn try_bootstrap(&self) -> Result<()> {
        self.join_network(true).await?;
        self.start().await
    }

    /// Run a one-shot demo command. The bootstrap/peer daemon should already be
    /// running elsewhere; this process joins the network, performs the command,
    /// prints the result, and exits.
    pub async fn run_command(&self) -> Result<()> {
        self.join_network(false).await?;

        match &self.command {
            Command::Serve => self.start().await,
            Command::CreateAuction {
                object,
                initial_value,
            } => {
                let auction = Auction::new(object.clone(), *initial_value, &self.node.node_info.id);
                self.publish_auction(&auction).await?;
                println!(
                    "Created auction\nkey: {}\nobject: {}\ninitial_value: {}\nseller: {}",
                    auction.key, auction.object, auction.initial_value, auction.seller
                );
                Ok(())
            }
            Command::FindAuction { key } => {
                let auction = self
                    .find_auction_in_network(key.clone())
                    .await?
                    .ok_or_else(|| anyhow!("Auction not found for key {}", key))?;
                print_auction(&auction);
                Ok(())
            }
            Command::Bid { key, amount } => {
                let mut auction = self
                    .find_auction_in_network(key.clone())
                    .await?
                    .ok_or_else(|| anyhow!("Auction not found for key {}", key))?;
                auction.place_bid(self.node.node_info.id.clone(), *amount)?;
                self.publish_auction(&auction).await?;
                println!(
                    "Placed bid\nauction: {}\namount: {}\nbuyer: {}",
                    auction.key, amount, self.node.node_info.id
                );
                Ok(())
            }
        }
    }

    async fn join_network(&self, discover_peers: bool) -> Result<()> {
        if self.node.node_info.port == self.node.config.bootstrap_peer_port {
            info!("This is the bootstrap node, skipping bootstrap...");
            return Ok(());
        }

        for attempt in 0..self.node.config.n_max_retries {
            info!("Attempt {} to bootstrap...", attempt + 1);

            let (challenge_hash_string, difficulty) = match self
                .bootstrap(
                    self.node.config.bootstrap_peer_ip.clone(),
                    self.node.config.bootstrap_peer_port,
                )
                .await
            {
                Ok(result) => result,
                Err(e) => {
                    error!("Bootstrap request failed: {:?}", e);
                    sleep_millis(self.node.config.peer_sync_ms);
                    continue;
                }
            };

            info!("Solving bootstrap challenge...");
            let challenge_solution = proof_of_work(&challenge_hash_string, difficulty);

            let response = self
                .challenge_resolution(
                    self.node.config.bootstrap_peer_ip.clone(),
                    self.node.config.bootstrap_peer_port,
                    challenge_solution,
                )
                .await;

            match response {
                Ok((true, bootstrap_node_info)) => {
                    info!("Bootstrap challenge solved successfully.");
                    // Insert bootstrap node into routing table after succesfully solving challenge.
                    self.node.insert_node_to_routing_table(bootstrap_node_info);
                    if discover_peers {
                        // Long-running peers discover neighbors. One-shot
                        // command clients skip this so the routing table does
                        // not fill with ports that exit immediately.
                        self.iterative_find_node(self.node.node_info.id.clone())
                            .await
                            .with_context(|| {
                                format!("Failed to find_node {}", self.node.node_info.id)
                            })?;
                    }

                    return Ok(());
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

    async fn publish_auction(&self, auction: &Auction) -> Result<()> {
        self.node.store_auction(auction.clone())?;

        if self.node.node_info.port != self.node.config.bootstrap_peer_port {
            self.send_store_to_node(
                self.node.config.bootstrap_peer_ip.clone(),
                self.node.config.bootstrap_peer_port,
                auction,
            )
            .await?;
        }

        Ok(())
    }

    async fn run_automation_tick(&self) -> Result<()> {
        let closed = self
            .node
            .close_expired_auctions(Utc::now().timestamp_millis())?;
        for auction in closed {
            self.publish_auction(&auction).await?;
            if let (Some(winner), Some(amount)) = (&auction.winner, auction.winning_bid) {
                info!(
                    "Closed auction {} won by {} for {}",
                    auction.key, winner, amount
                );
            }
        }

        let action = rand::thread_rng().gen_range(0..3);
        match action {
            0 => self.create_random_auction().await?,
            1 => self.bid_on_random_auction().await?,
            _ => {}
        }

        Ok(())
    }

    async fn create_random_auction(&self) -> Result<()> {
        let items = [
            "Vintage keyboard",
            "Mechanical watch",
            "Signed poster",
            "Film camera",
            "Studio headphones",
            "Arcade cabinet",
            "Antique map",
            "Handmade chess set",
        ];
        let mut rng = rand::thread_rng();
        let object = items
            .choose(&mut rng)
            .ok_or_else(|| anyhow!("No automation items configured"))?
            .to_string();
        let initial_value = rng.gen_range(10..150);
        let auction = Auction::new_with_duration(
            object,
            initial_value,
            &self.node.node_info.id,
            self.node.config.auction_duration_ms,
        );

        self.publish_auction(&auction).await?;
        info!(
            "Automation created auction {} with initial value {}",
            auction.key, auction.initial_value
        );
        Ok(())
    }

    async fn bid_on_random_auction(&self) -> Result<()> {
        let mut auctions = self
            .node
            .all_auctions()?
            .into_iter()
            .filter(|auction| {
                auction.status == AuctionStatus::Open && auction.seller != self.node.node_info.id
            })
            .collect::<Vec<_>>();

        if auctions.is_empty() {
            return Ok(());
        }

        let mut rng = rand::thread_rng();
        let auction = auctions
            .choose_mut(&mut rng)
            .ok_or_else(|| anyhow!("No auction available to bid on"))?;
        let amount = auction.highest_bid_amount() + rng.gen_range(1..25);
        auction.place_bid(self.node.node_info.id.clone(), amount)?;
        self.publish_auction(auction).await?;
        info!("Automation bid {} on auction {}", amount, auction.key);
        Ok(())
    }

    async fn find_auction_in_network(&self, key: Address) -> Result<Option<Auction>> {
        if let Some(auction) = self.node.find_auction(&key)? {
            return Ok(Some(auction));
        }

        let mut nodes_to_query = VecDeque::new();
        nodes_to_query.push_back(NodeInfo {
            id: key.clone(),
            ip: self.node.config.bootstrap_peer_ip.clone(),
            port: self.node.config.bootstrap_peer_port,
        });

        let mut queried = HashSet::new();
        while let Some(node) = nodes_to_query.pop_front() {
            let query_key = format!("{}:{}", node.ip, node.port);
            if !queried.insert(query_key) {
                continue;
            }

            let response = self
                .find_value(node.ip.clone(), node.port, key.clone())
                .await
                .with_context(|| {
                    format!("Failed to query {}:{} for auction", node.ip, node.port)
                })?;

            match response.result {
                Some(find_value_response::Result::Value(value)) => {
                    let proto = crate::g_rpc::kademlia::Auction::decode(&*value.value)
                        .with_context(|| "Failed to decode auction value")?;
                    return Ok(Some(Auction::try_from(&proto)?));
                }
                Some(find_value_response::Result::Nodes(nodes)) => {
                    for node_info in nodes.nodes {
                        let node = NodeInfo::try_from(&node_info)?;
                        if node.id != self.node.node_info.id {
                            nodes_to_query.push_back(node);
                        }
                    }
                }
                None => {}
            }
        }

        Ok(None)
    }

    // Useful for manual demos and health checks even though the automatic
    // startup path currently goes straight to bootstrap/FIND_NODE.
    #[allow(dead_code)]
    pub async fn ping(&self, node_ip: String, node_port: u32) -> Result<()> {
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

        drop(client);
        Ok(())
    }

    // Exposes the value lookup half of Kademlia for future CLI/API commands.
    #[allow(dead_code)]
    pub async fn find_value(
        &self,
        node_ip: String,
        node_port: u32,
        key: Address,
    ) -> Result<FindValueResponse> {
        let target = generate_url(&node_ip, node_port);

        let mut client = KademliaClient::connect(target.clone())
            .await
            .with_context(|| format!("Failed to connect to {} for find_value", target))?;

        let inner_request = crate::g_rpc::kademlia::FindValueRequest {
            key: key.to_string(),
        };

        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(),
            &inner_request,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        )
        .with_context(|| "Failed to sign find_value request")?;

        let response = client
            .find_value(Request::new(auth_msg))
            .await
            .with_context(|| "Failed to send find_value request")?
            .into_inner();

        let (payload, _) = extract_and_verify::<FindValueResponse>(response)
            .await
            .with_context(|| "Failed to verify find_value response")?;

        Ok(payload)
    }

    async fn bootstrap(
        &self,
        bootstrap_node_ip: String,
        bootstrap_node_port: u32,
    ) -> Result<(String, u32)> {
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

        drop(client);
        Ok((payload.hash, payload.difficulty))
    }

    async fn challenge_resolution(
        &self,
        bootstrap_node_ip: String,
        bootstrap_node_port: u32,
        challenge_resolution: u64,
    ) -> Result<(bool, NodeInfo)> {
        let target = generate_url(&bootstrap_node_ip, bootstrap_node_port);
        let mut client = KademliaClient::connect(target.clone())
            .await
            .with_context(|| format!("Failed to connect to {}", target))?;

        let inner_request = ChallengeResolutionRequest {
            nonce: challenge_resolution,
        };

        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(),
            &inner_request,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        )
        .with_context(|| "Failed to sign challenge resolution request")?;

        let request = Request::new(auth_msg);

        let response = client
            .challenge_resolution(request)
            .await
            .with_context(|| "Failed to send challenge resolution request")?
            .into_inner();

        let (payload, parsed_response) =
            extract_and_verify::<ChallengeResolutionResponse>(response)
                .await
                .with_context(|| "Failed to extract/verify challenge resolution response")?;

        let sender = parsed_response
            .sender
            .ok_or_else(|| anyhow!("Missing sender info in challenge response"))?;

        info!("Received challenge resolution response from bootstrap node");

        drop(client);
        Ok((payload.accepted, NodeInfo::try_from(&sender)?))
    }

    async fn find_node(
        &self,
        node_ip: String,
        node_port: u32,
        target_id: Address,
    ) -> Result<Vec<NodeInfo>> {
        let target = generate_url(&node_ip, node_port);

        let mut client = KademliaClient::connect(target.clone())
            .await
            .with_context(|| format!("Failed to connect to {} for find_node", target))?;

        let inner_request = FindNodeRequest {
            target_id: target_id.to_string(),
        };

        let auth_msg = sign_and_wrap(
            self.node.node_info.clone(),
            &inner_request,
            self.node.config.private_key.clone(),
            self.node.config.public_key.clone(),
        )
        .with_context(|| "Failed to sign find_node request")?;

        let request = Request::new(auth_msg);

        let response = client
            .find_node(request)
            .await
            .with_context(|| "Failed to send find_node request")?
            .into_inner();

        let (payload, _) = extract_and_verify::<FindNodeResponse>(response)
            .await
            .with_context(|| "Failed to verify find_node response")?;

        let closest_nodes = payload
            .closest_nodes
            .into_iter()
            .map(|node_info| Ok(NodeInfo::try_from(&node_info)?))
            .collect::<Result<Vec<NodeInfo>>>()?;

        drop(client);
        Ok(closest_nodes)
    }

    pub async fn iterative_find_node(&self, target_id: Address) -> Result<()> {
        let mut queried = HashSet::new();
        let mut shortlist = VecDeque::new();
        let mut found_nodes = Vec::new();

        // 1. Seed initial shortlist with known nodes (e.g., from your routing table).
        let mut closest_nodes: Vec<NodeInfo> = vec![];
        self.node
            .routing_table
            .get_k_closest_nodes(&target_id, &mut closest_nodes);
        for node in closest_nodes {
            shortlist.push_back(node);
        }

        // 2. Main loop
        while let Some(node) = shortlist.pop_front() {
            // Already queried.
            if queried.contains(&node.id) {
                continue;
            }

            // Mark as queried.
            queried.insert(node.id.clone());

            info!("FIND_NODE for {}:{}", &node.ip, &node.port);
            let result = self
                .find_node(node.ip.clone(), node.port, target_id.clone())
                .await;

            let new_nodes = match result {
                Ok(nodes) => nodes,
                Err(err) => {
                    // Skip on error.
                    warn!("Failed to query node {}:{}: {:?}", node.ip, node.port, err);
                    continue;
                }
            };

            for new_node in new_nodes {
                if queried.contains(&new_node.id) {
                    continue;
                }

                if new_node.id == self.node.node_info.id {
                    continue;
                }

                shortlist.push_back(new_node.clone());

                if !(found_nodes.iter().any(|n: &NodeInfo| n.id == node.id)) {
                    found_nodes.push(new_node.clone());
                    self.node.insert_node_to_routing_table(new_node);
                }
            }

            shortlist
                .make_contiguous()
                .sort_by_key(|n| n.id.distance(&target_id));
        }

        Ok(())
    }

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

fn print_auction(auction: &Auction) {
    println!(
        "Auction\nkey: {}\nobject: {}\ninitial_value: {}\ncurrent_price: {}\nseller: {}\nbids: {}",
        auction.key,
        auction.object,
        auction.initial_value,
        auction.highest_bid_amount(),
        auction.seller,
        auction.bids.len()
    );

    for bid in &auction.bids {
        println!("bid: buyer={} amount={}", bid.buyer, bid.amount);
    }
}
