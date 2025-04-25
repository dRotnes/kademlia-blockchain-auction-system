#[macro_use]
extern crate log;

mod node;
mod utils;
mod routing;
mod g_rpc;

// External libraries
use local_ip_address::local_ip;
use node::Node;
use g_rpc::{SKademliaServer, SKademliaClient};
use utils::context::Context;

use crate::utils::{
    termination,
    logger,
    execution,
    Config,
    crypto_own::{hash_data, setup_keys}
};

fn main() {
    logger::initialize_logger();

    // Quit the program when the user inputs Ctrl-C.
    termination::set_ctrlc_handler();

    // Read environment variables and setup config object.
    let mut config: Config = Config::read();
    // Setup the public and private keys.
    setup_keys(&mut config);

    // Setup node_id.
    let node_id = hash_data(&config.public_key);
    info!("Node id generated: {}", &node_id);
    // Get local IP.
    // let my_local_ip= local_ip().unwrap().to_string();
    let my_local_ip= String::from("127.0.0.1");

    // Setup context.
    let context = Context {
        // NOTE: The last one should get the original config, the others a clone.
        node: Node::new(node_id, my_local_ip, config.port, config),
    };

    
    let server = SKademliaServer::new(&context);
    let client = SKademliaClient::new(&context);
    execution::run_in_parallel(vec![&server, &client])

}
