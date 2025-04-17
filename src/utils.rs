mod config;
pub mod logger;
pub mod execution;
pub mod termination;
pub mod context;

use std::{fs::{self, File}, io::{BufWriter, Write}};

pub use config::Config;

use ethereum_types::U256;
use crypto::{digest::Digest, sha2::Sha256};
use openssl::{pkey::PKey, rsa::Rsa};
use rand::{thread_rng, Rng};

/**
 * Resolves a proof of work challenge.
 * Given an input and difficulty, finds a nonce that produces a hash with enough leading zeros.
 */
pub fn proof_of_work(input: &str, difficulty: u32) -> u64 {
    let mut nonce: u64 = 0;

    loop {
        let data_to_hash = format!("{}{}", input, nonce);
        let solved_hash = hash_data(&data_to_hash);

        if solved_hash.leading_zeros() >= difficulty {
            return nonce;
        }

        nonce += 1;
    }
}

/**
 * Hashes input data using SHA-256.
 */
pub fn hash_data(input: &str) -> U256 {
    let mut byte_hash = <[u8; 32]>::default();
    let mut hasher = Sha256::new();

    hasher.input_str(input);
    hasher.result(&mut byte_hash);

    U256::from(&byte_hash)
}

/**
 * Generates a challenge according to the difficulty.
 */
pub fn generate_challenge(difficulty: u32) -> U256 {
    let mut rng = thread_rng();

    // Generate a random 256-bit number
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes);

    U256::from(&bytes)
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
 * Calculates the XOR distance metric between two node ids.
 */
pub fn calculate_distance(id1: U256, id2: U256) -> U256 {
    id1 ^id2
}


