#[macro_use]
extern crate log;

mod auction;
mod blockchain;
mod command;
mod consensus;
mod g_rpc;
mod node;
mod routing;
mod storage;
mod utils;

use std::env;
use std::str::FromStr;

use blockchain::address::Address;
use command::Command;
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
    let command = Command::parse(&env::args().collect::<Vec<_>>()).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });

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
    let client = SKademliaClient::new(&context, command.clone());

    if command.is_serve() {
        execution::run_in_parallel(vec![&server, &client])
    } else {
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        if let Err(error) = runtime.block_on(client.run_command()) {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
    }
}
