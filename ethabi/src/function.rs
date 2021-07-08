// Copyright 2015-2020 Parity Technologies
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Contract function call builder.

use std::string::ToString;

use crate::{
	decode, encode, signature::short_signature, Bytes, Error, Param, ParamType, Result, StateMutability, Token,
};
use serde::{Deserialize, Serialize};

/// Contract function specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Function {
	/// Function name.
	#[serde(deserialize_with = "crate::util::sanitize_name::deserialize")]
	pub name: String,
	/// Function input.
	pub inputs: Vec<Param>,
	/// Function output.
	pub outputs: Vec<Param>,
	#[deprecated(note = "The constant attribute was removed in Solidity 0.5.0 and has been \
				replaced with stateMutability. If parsing a JSON AST created with \
				this version or later this value will always be false, which may be wrong.")]
	/// Constant function.
	#[serde(default)]
	pub constant: bool,
	/// Whether the function reads or modifies blockchain state
	#[serde(rename = "stateMutability", default)]
	pub state_mutability: StateMutability,
}

impl Function {
	/// Returns all input params of given function.
	fn input_param_types(&self) -> Vec<ParamType> {
		self.inputs.iter().map(|p| p.kind.clone()).collect()
	}

	/// Returns all output params of given function.
	fn output_param_types(&self) -> Vec<ParamType> {
		self.outputs.iter().map(|p| p.kind.clone()).collect()
	}

	/// Prepares ABI function call with given input params.
	pub fn encode_input(&self, tokens: &[Token]) -> Result<Bytes> {
		let params = self.input_param_types();

		if !Token::types_check(tokens, &params) {
			return Err(Error::InvalidData);
		}

		let signed = short_signature(&self.name, &params).to_vec();
		let encoded = encode(tokens);
		Ok(signed.into_iter().chain(encoded.into_iter()).collect())
	}

	/// Parses the ABI function output to list of tokens.
	pub fn decode_output(&self, data: &[u8]) -> Result<Vec<Token>> {
		decode(&self.output_param_types(), &data)
	}

	/// Parses the ABI function input to a list of tokens.
	pub fn decode_input(&self, data: &[u8]) -> Result<Vec<Token>> {
		decode(&self.input_param_types(), &data)
	}

	/// Returns a signature that uniquely identifies this function.
	///
	/// Examples:
	/// - `functionName()`
	/// - `functionName():(uint256)`
	/// - `functionName(bool):(uint256,string)`
	/// - `functionName(uint256,bytes32):(string,uint256)`
	pub fn signature(&self) -> String {
		let inputs = self.inputs.iter().map(|p| p.kind.to_string()).collect::<Vec<_>>().join(",");

		let outputs = self.outputs.iter().map(|p| p.kind.to_string()).collect::<Vec<_>>().join(",");

		match (inputs.len(), outputs.len()) {
			(_, 0) => format!("{}({})", self.name, inputs),
			(_, _) => format!("{}({}):({})", self.name, inputs, outputs),
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::{Function, Param, ParamType, StateMutability, Token};
	use hex_literal::hex;

	#[test]
	fn test_function_encode_call() {
		#[allow(deprecated)]
		let func = Function {
			name: "baz".to_owned(),
			inputs: vec![
				Param { name: "a".to_owned(), kind: ParamType::Uint(32) },
				Param { name: "b".to_owned(), kind: ParamType::Bool },
			],
			outputs: vec![],
			constant: false,
			state_mutability: StateMutability::Payable,
		};

		let mut uint = [0u8; 32];
		uint[31] = 69;
		let encoded = func.encode_input(&[Token::Uint(uint.into()), Token::Bool(true)]).unwrap();
		let expected = hex!("cdcd77c000000000000000000000000000000000000000000000000000000000000000450000000000000000000000000000000000000000000000000000000000000001").to_vec();
		assert_eq!(encoded, expected);
	}
}
