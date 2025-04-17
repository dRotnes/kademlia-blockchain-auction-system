mod server;
mod client;
pub mod kademlia {
    tonic::include_proto!("kademlia");
}

pub use server::SKademliaServer;
pub use client::SKademliaClient;