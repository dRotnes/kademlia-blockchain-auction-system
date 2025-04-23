use openssl::hash::MessageDigest;
use openssl::{pkey::PKey, rsa::Rsa};
use openssl::sign::{Signer, Verifier};
use prost::Message;

use crate::gRPC::kademlia::{AuthenticatedMessage, NodeInfo};
use crate::node;

/// Signs a protobuf message and wraps it in an AuthenticatedMessage
pub fn sign_and_wrap<M: Message>(
    node_info: node::NodeInfo,
    message: &M,
    private_key_pem: String,
    public_key_pem: String,
) -> Result<AuthenticatedMessage, anyhow::Error> {
    let private_key = PKey::private_key_from_pem(private_key_pem.as_bytes())?;
    let public_key = Rsa::public_key_from_pem(public_key_pem.as_bytes())?;
    let public_key_der = public_key.public_key_to_der()?; 

    // Serialize protobuf message to bytes
    let mut message_bytes = Vec::new();
    message.encode(&mut message_bytes)?;

    // Sign the message
    let mut signer = Signer::new_without_digest(&private_key)?;
    signer.update(&message_bytes)?;
    let signature = signer.sign_to_vec()?;

    // Return wrapped message
    Ok(AuthenticatedMessage {
        sender: Some( NodeInfo {
            id: node_info.id.clone().to_string(),
            ip: node_info.ip.clone(),
            port: node_info.port
        }),
        public_key: public_key_der,
        signature,
        payload: message_bytes,
    })
}

/// Verifies the signature of a message given the raw message, signature, and DER-encoded public key.
pub fn verify_signature(
    payload: &[u8],
    signature: &[u8],
    public_key_der: &[u8],
) -> Result<bool, anyhow::Error> {
    // Parse public key
    let public_key = PKey::public_key_from_der(public_key_der)?;

    // Create verifier with SHA256 digest
    let mut verifier = Verifier::new(MessageDigest::sha256(), &public_key)?;
    verifier.update(payload)?;

    // Verify the signature
    Ok(verifier.verify(signature)?)
}