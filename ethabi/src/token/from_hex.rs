//! Creates fixed size token from bytes.

use hex::FromHex;
use errors::{Error, ErrorKind};

/// Creates fixed size token from bytes.
pub trait TokenFromHex<T> {
	/// Converts bytes to token.
	fn token_from_hex(&self) -> Result<T, Error>;
}

macro_rules! impl_token_from_hex {
	($size: expr) => {
		impl TokenFromHex<[u8; $size]> for str {
			fn token_from_hex(&self) -> Result<[u8; $size], Error> {
				let mut result = [0u8; $size];
				let bytes = self.from_hex()?;

				if bytes.len() != $size {
					return Err(ErrorKind::InvalidData.into());
				}

				result.copy_from_slice(&bytes);
				Ok(result)
			}
		}
	}
}

impl_token_from_hex!(20);
impl_token_from_hex!(32);
