use std::{fs, path::Path};

use anyhow::{Context, Result};

use crate::{auction::Auction, blockchain::block::Block};

const AUCTIONS_FILE: &str = "auctions.json";
const LEDGER_FILE: &str = "ledger.json";

pub fn load_auctions(data_dir: &str) -> Result<Vec<Auction>> {
    read_json(&Path::new(data_dir).join(AUCTIONS_FILE))
}

pub fn save_auctions(data_dir: &str, auctions: &[Auction]) -> Result<()> {
    write_json(&Path::new(data_dir).join(AUCTIONS_FILE), auctions)
}

pub fn load_ledger(data_dir: &str) -> Result<Vec<Block>> {
    read_json(&Path::new(data_dir).join(LEDGER_FILE))
}

pub fn save_ledger(data_dir: &str, ledger: &[Block]) -> Result<()> {
    write_json(&Path::new(data_dir).join(LEDGER_FILE), ledger)
}

fn read_json<T>(path: &Path) -> Result<Vec<T>>
where
    T: serde::de::DeserializeOwned,
{
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&contents).with_context(|| format!("Failed to parse {}", path.display()))
}

fn write_json<T>(path: &Path, value: &[T]) -> Result<()>
where
    T: serde::Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    let contents = serde_json::to_string_pretty(value)?;
    fs::write(path, contents).with_context(|| format!("Failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::address::Address;
    use std::str::FromStr;

    #[test]
    fn saves_and_loads_auctions() {
        let unique_dir = format!(
            "./target/test/storage-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        );
        let seller =
            Address::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let auction = Auction::new("camera".to_string(), 20, &seller);

        save_auctions(&unique_dir, &[auction.clone()]).unwrap();
        let loaded = load_auctions(&unique_dir).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].key, auction.key);
    }
}
