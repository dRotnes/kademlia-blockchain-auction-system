mod client;
mod server;
pub mod kademlia {
    tonic::include_proto!("kademlia");
}

pub use client::SKademliaClient;
pub use server::SKademliaServer;
