//! Ethereum ABI params.
use std::fmt;
use spec::ParamType;
use hex::ToHex;

/// Ethereum ABI params.
#[derive(Debug, PartialEq, Clone)]
pub enum Token {
	/// Address.
	///
	/// solidity name: address
	/// Encoded to left padded [0u8; 32].
	Address([u8;20]),
	/// Vector of bytes with known size.
	///
	/// solidity name eg.: bytes8, bytes32, bytes64, bytes1024
	/// Encoded to right padded [0u8; ((N + 31) / 32) * 32].
	FixedBytes(Vec<u8>),
	/// Vector of bytes of unknown size.
	///
	/// solidity name: bytes
	/// Encoded in two parts.
	/// Init part: offset of 'closing part`.
	/// Closing part: encoded length followed by encoded right padded bytes.
	Bytes(Vec<u8>),
	/// Signed integer.
	///
	/// solidity name: int
	Int([u8;32]),
	/// Unisnged integer.
	///
	/// solidity name: uint
	Uint([u8;32]),
	/// Boolean value.
	///
	/// solidity name: bool
	/// Encoded as left padded [0u8; 32], where last bit represents boolean value.
	Bool(bool),
	/// String.
	///
	/// solidity name: string
	/// Encoded in the same way as bytes. Must be utf8 compliant.
	String(String),
	/// Array with known size.
	///
	/// solidity name eg.: int[3], bool[3], address[][8]
	/// Encoding of array is equal to encoding of consecutive elements of array.
	FixedArray(Vec<Token>),
	/// Array of params with unknown size.
	///
	/// solidity name eg. int[], bool[], address[5][]
	Array(Vec<Token>),
}

impl fmt::Display for Token {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			Token::Bool(b) => write!(f, "{}", b),
			Token::String(ref s) => write!(f, "{}", s),
			Token::Address(ref a) => write!(f, "{}", a.to_hex()),
			Token::Bytes(ref bytes) | Token::FixedBytes(ref bytes) => write!(f, "{}", bytes.to_hex()),
			Token::Uint(ref i) | Token::Int(ref i) => write!(f, "{}", i.to_hex()),
			Token::Array(ref arr) | Token::FixedArray(ref arr) => {
				let s = arr.iter()
					.map(|ref t| format!("{}", t))
					.collect::<Vec<String>>()
					.join(",");

				write!(f, "[{}]", s)
			}
		}
	}
}

impl Token {
	/// Check whether the type of the token matches the given parameter type.
	///
	/// Numeric types (`Int` and `Uint`) type check if the size of the token
	/// type is of greater or equal size than the provided parameter type.
	pub fn type_check(&self, param_type: &ParamType) -> bool {
		match *self {
			Token::Address(_) => *param_type == ParamType::Address,
			Token::Bytes(_) => *param_type == ParamType::Bytes,
			Token::Int(bytes) =>
				if let ParamType::Int(size) = *param_type {
					size <= bytes.len() * 8
				} else {
					false
				},
			Token::Uint(bytes) =>
				if let ParamType::Uint(size) = *param_type {
					size <= bytes.len() * 8
				} else {
					false
				},
			Token::Bool(_) => *param_type == ParamType::Bool,
			Token::String(_) => *param_type == ParamType::String,
			Token::FixedBytes(ref bytes) =>
				if let ParamType::FixedBytes(size) = *param_type {
					size == bytes.len()
				} else {
					false
				},
			Token::Array(ref tokens) =>
				if let ParamType::Array(ref param_type) = *param_type {
					tokens.iter().all(|t| t.type_check(param_type))
				} else {
					false
				},
			Token::FixedArray(ref tokens) =>
				if let ParamType::FixedArray(ref param_type, size) = *param_type {
					size == tokens.len() && tokens.iter().all(|t| t.type_check(param_type))
				} else {
					false
				},
		}
	}

	/// Converts token to...
	pub fn to_address(self) -> Option<[u8; 20]> {
		match self {
			Token::Address(address) => Some(address),
			_ => None,
		}
	}

	/// Converts token to...
	pub fn to_fixed_bytes(self) -> Option<Vec<u8>> {
		match self {
			Token::FixedBytes(bytes) => Some(bytes),
			_ => None,
		}
	}

	/// Converts token to...
	pub fn to_bytes(self) -> Option<Vec<u8>> {
		match self {
			Token::Bytes(bytes) => Some(bytes),
			_ => None,
		}
	}

	/// Converts token to...
	pub fn to_int(self) -> Option<[u8; 32]> {
		match self {
			Token::Int(int) => Some(int),
			_ => None,
		}
	}

	/// Converts token to...
	pub fn to_uint(self) -> Option<[u8; 32]> {
		match self {
			Token::Uint(uint) => Some(uint),
			_ => None,
		}
	}

	/// Converts token to...
	pub fn to_bool(self) -> Option<bool> {
		match self {
			Token::Bool(b) => Some(b),
			_ => None,
		}
	}

	/// Converts token to...
	pub fn to_string(self) -> Option<String> {
		match self {
			Token::String(s) => Some(s),
			_ => None,
		}
	}

	/// Converts token to...
	pub fn to_fixed_array(self) -> Option<Vec<Token>> {
		match self {
			Token::FixedArray(arr) => Some(arr),
			_ => None,
		}
	}

	/// Converts token to...
	pub fn to_array(self) -> Option<Vec<Token>> {
		match self {
			Token::Array(arr) => Some(arr),
			_ => None,
		}
	}
}
