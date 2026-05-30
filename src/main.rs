#[macro_use]
extern crate log;

mod auction;
mod blockchain;
mod g_rpc;
mod node;
mod routing;
mod utils;

use std::str::FromStr;

use blockchain::address::Address;
use g_rpc::{SKademliaClient, SKademliaServer};
use node::Node;
use utils::context::Context;

use crate::utils::{
    crypto_own::{hash_data, setup_keys},
    execution, format_as_hex_string, logger, termination, Config,
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
    let address = Address::from_str(&format_as_hex_string(node_id)).unwrap();

    // Get local IP.
    // let my_local_ip= local_ip().unwrap().to_string();
    let my_local_ip = String::from("127.0.0.1");

    // Setup context.
    let context = Context {
        // NOTE: The last one should get the original config, the others a clone.
        node: Node::new(address, my_local_ip, config.port, config),
    };

    let server = SKademliaServer::new(&context);
    let client = SKademliaClient::new(&context);
    execution::run_in_parallel(vec![&server, &client])
}
