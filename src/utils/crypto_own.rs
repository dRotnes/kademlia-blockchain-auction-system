use ethereum_types::U256;
use openssl::hash::MessageDigest;
use openssl::{pkey::PKey, rsa::Rsa};
use openssl::sign::{Signer, Verifier};
use prost::Message;
use sha2::{Sha256, Digest};
use tonic::Status;
use std::{fs::{self, File}, io::{BufWriter, Write}};

use crate::g_rpc::kademlia::{AuthenticatedMessage, NodeInformation};
use crate::node;

use super::{format_as_hex_string, Config};


/**
 * Hashes input data using SHA-256.
 */
pub fn hash_data(input: &str) -> U256 {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let byte_hash = hasher.finalize();

    U256::from_big_endian(&byte_hash)
}

/**
 * Sings a message and wraps it in an Authenticated message.
 */
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
        sender: Some( NodeInformation {
            id: format_as_hex_string(node_info.id),
            ip: node_info.ip,
            port: node_info.port
        }),
        public_key: public_key_der,
        signature,
        payload: message_bytes,
    })
}

/**
 * Verifies a message signature.
 */
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

/**
 * Generates a private and public key pair if none existent.
 */
pub fn create_rsa_key_pair() {
    let rsa = Rsa::generate(2048).expect("Failed to generate RSA key");
    let pkey = PKey::from_rsa(rsa).expect("Failed to create PKey");

    // Save private key to file
    let private_key_pem = pkey.private_key_to_pem_pkcs8().expect("Failed to get private key PEM");
    let mut private_file = BufWriter::new(File::create("private_key.pem").expect("Failed to create private key file"));
    private_file.write_all(&private_key_pem).expect("Failed to write private key");
    
    // Save public key to file
    let pub_key_pem = pkey.public_key_to_pem().expect("Failed to get public key PEM");
    let mut public_file = BufWriter::new(File::create("public_key.pem").expect("Failed to create public key file"));
    public_file.write_all(&pub_key_pem).expect("Failed to write public key");
}

/**
 * Generates a public key from an existing private key and saves it.
 */
pub fn generate_public_key_from_private(private_key_pem: &str) {
    // Read private key
    let rsa = Rsa::private_key_from_pem(private_key_pem.as_bytes()).expect("Failed to parse private key");
    let pkey = PKey::from_rsa(rsa).expect("Failed to create PKey from private key");

    // Generate and save public key
    let pub_key_pem = pkey.public_key_to_pem().expect("Failed to get public key PEM");
    let mut public_file = BufWriter::new(File::create("public_key.pem").expect("Failed to create public key file"));
    public_file.write_all(&pub_key_pem).expect("Failed to write public key");
}

/**
 * Sets up the keys.
 */
pub fn setup_keys(config: &mut Config) {
    info!("Setting up keys...");
    if config.private_key.is_empty() && config.public_key.is_empty() {
        info!("No private key provided, generating private and public key");
        create_rsa_key_pair();
        config.private_key = fs::read_to_string("private_key.pem").expect("Failed to read private key file");
        config.public_key = fs::read_to_string("public_key.pem").expect("Failed to read public key file");
        info!("Generated private and public keys.");
    } else if config.private_key.is_empty() {
        panic!("Public key is provided but private key is missing! Aborting.");
    } else if config.public_key.is_empty() {
        info!("Private key provided but public key is missing. Generating public key.");
        generate_public_key_from_private(&config.private_key);
        config.public_key = fs::read_to_string("public_key.pem").expect("Failed to read public key file");
        info!("Generated public key.");
    }
}

/**
 * Extracts and verifies a message.
 */
pub async fn extract_and_verify<T: prost::Message + Default>(
    msg: AuthenticatedMessage,
) -> Result<(T, AuthenticatedMessage), Status> {
    let is_valid = verify_signature(&msg.payload, &msg.signature, &msg.public_key)
        .map_err(|e| Status::unauthenticated(format!("Signature check error: {:?}", e)))?;

    if !is_valid {
        return Err(Status::unauthenticated("Invalid signature"));
    }

    let payload = T::decode(&*msg.payload)
        .map_err(|e| Status::invalid_argument(format!("Failed to decode message: {:?}", e)))?;

    Ok((payload, msg))
}