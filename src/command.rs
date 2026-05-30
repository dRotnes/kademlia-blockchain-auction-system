use std::str::FromStr;

use anyhow::{anyhow, Result};

use crate::blockchain::address::Address;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Serve,
    CreateAuction { object: String, initial_value: u64 },
    FindAuction { key: Address },
    Bid { key: Address, amount: u64 },
}

impl Command {
    /// Parse the application command while ignoring global flags handled by
    /// Config. Keeping this parser tiny avoids another dependency and makes the
    /// demo commands easy to audit.
    pub fn parse(args: &[String]) -> Result<Command> {
        let positional = strip_global_flags(args)?;

        match positional.as_slice() {
            [] => Ok(Command::Serve),
            [command, object, initial_value] if command == "create-auction" => {
                Ok(Command::CreateAuction {
                    object: object.clone(),
                    initial_value: initial_value.parse::<u64>().map_err(|_| {
                        anyhow!("create-auction requires an integer initial value")
                    })?,
                })
            }
            [command, key] if command == "find-auction" => Ok(Command::FindAuction {
                key: Address::from_str(key).map_err(|_| anyhow!("Invalid auction key"))?,
            }),
            [command, key, amount] if command == "bid" => Ok(Command::Bid {
                key: Address::from_str(key).map_err(|_| anyhow!("Invalid auction key"))?,
                amount: amount
                    .parse::<u64>()
                    .map_err(|_| anyhow!("bid requires an integer amount"))?,
            }),
            _ => Err(anyhow!(
                "Usage: --port <PORT> [create-auction <OBJECT> <INITIAL_VALUE> | find-auction <KEY> | bid <KEY> <AMOUNT>]"
            )),
        }
    }

    pub fn is_serve(&self) -> bool {
        matches!(self, Command::Serve)
    }
}

fn strip_global_flags(args: &[String]) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--port" => {
                if index + 1 >= args.len() {
                    return Err(anyhow!("Missing value for --port"));
                }
                index += 2;
            }
            arg => {
                result.push(arg.to_string());
                index += 1;
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parses_default_serve_command() {
        let command = Command::parse(&args(&["app", "--port", "8000"])).unwrap();
        assert_eq!(command, Command::Serve);
    }

    #[test]
    fn parses_create_auction_command() {
        let command = Command::parse(&args(&[
            "app",
            "--port",
            "8001",
            "create-auction",
            "Poster",
            "25",
        ]))
        .unwrap();

        assert_eq!(
            command,
            Command::CreateAuction {
                object: "Poster".to_string(),
                initial_value: 25
            }
        );
    }

    #[test]
    fn rejects_invalid_bid_amount() {
        let result = Command::parse(&args(&[
            "app",
            "--port",
            "8002",
            "bid",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "too-much",
        ]));

        assert!(result.is_err());
    }
}
