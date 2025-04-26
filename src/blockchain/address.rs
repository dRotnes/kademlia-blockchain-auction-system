use std::{
    convert::{TryFrom, TryInto},
    fmt,
    str::FromStr,
};

use ethereum_types::U256;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// Addresses are 32-bytes long
type Byte = u8;
const LEN: usize = 32;

#[derive(Error, PartialEq, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum AddressError {
    #[error("Invalid format")]
    InvalidFormat,

    #[error("Invalid length")]
    InvalidLength,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct Address([Byte; LEN]);

impl Address {
    pub fn as_u256(&self) -> U256 {
        U256::from_big_endian(&self.0)
    }

    pub fn distance(&self, other: &Address) -> U256 {
        self.as_u256() ^ other.as_u256()
    }
}

impl TryFrom<Vec<Byte>> for Address {
    type Error = AddressError;

    fn try_from(vec: Vec<Byte>) -> Result<Self, AddressError> {
        let slice = vec.as_slice();
        match slice.try_into() {
            Ok(byte_array) => Ok(Address(byte_array)),
            Err(_) => Err(AddressError::InvalidLength),
        }
    }
}

impl TryFrom<String> for Address {
    type Error = AddressError;

    fn try_from(s: String) -> Result<Self, AddressError> {
        match hex::decode(s) {
            Ok(decoded_vec) => decoded_vec.try_into(),
            Err(_) => Err(AddressError::InvalidFormat),
        }
    }
}

impl FromStr for Address {
    type Err = AddressError;

    fn from_str(s: &str) -> Result<Self, AddressError> {
        Address::try_from(s.to_string())
    }
}

impl From<Address> for String {
    fn from(account: Address) -> Self {
        account.to_string()
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}
