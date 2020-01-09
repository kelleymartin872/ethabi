mod error;

use structopt::StructOpt;
use std::fs::File;
use rustc_hex::{ToHex, FromHex};
use ethabi::param_type::{ParamType, Reader};
use ethabi::token::{Token, Tokenizer, StrictTokenizer, LenientTokenizer};
use ethabi::{encode, decode, Contract, Function, Event, Hash};
use itertools::Itertools;
use crate::error::Error;
use tiny_keccak::Keccak;

#[derive(StructOpt, Debug)]
/// Ethereum ABI coder.
///
/// Copyright 2016-2019 Parity Technologies (UK) Limited
enum Opt {
	/// Encode ABI call.
	Encode(Encode),
	/// Decode ABI call result.
	Decode(Decode),
}

#[derive(StructOpt, Debug)]
enum Encode {
	/// Load function from JSON ABI file.
	Function {
		abi_path: String,
		function_name_or_signature: String,
		#[structopt(short, number_of_values = 1)]
		params: Vec<String>,
		/// Allow short representation of input params.
		#[structopt(short, long)]
		lenient: bool,
	},
	/// Specify types of input params inline.
	Params {
		/// Pairs of types directly followed by params in the form:
		///
		/// -v <type1> <param1> -v <type2> <param2> ...
		#[structopt(short = "v", name = "type-or-param", number_of_values = 2, allow_hyphen_values = true)]
		params: Vec<String>,
		/// Allow short representation of input params.
		#[structopt(short, long)]
		lenient: bool,
	},
}

#[derive(StructOpt, Debug)]
enum Decode {
	/// Load function from JSON ABI file.
	Function {
		abi_path: String,
		function_name_or_signature: String,
		data: String,
	},
	/// Specify types of input params inline.
	Params {
		#[structopt(short, name = "type", number_of_values = 1)]
		types: Vec<String>,
		data: String,
	},
	/// Decode event log.
	Log {
		abi_path: String,
		event_name_or_signature: String,
		#[structopt(short = "l", name = "topic", number_of_values = 1)]
		topics: Vec<String>,
		data: String,
	},
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	println!("{}", execute(std::env::args())?);

	Ok(())
}

fn execute<I>(args: I) -> Result<String, Error>
where
	I: IntoIterator,
	I::Item: Into<std::ffi::OsString> + Clone
{
	let opt = Opt::from_iter(args);

	match opt {
		Opt::Encode(Encode::Function { abi_path, function_name_or_signature, params, lenient }) =>
			encode_input(&abi_path, &function_name_or_signature, &params, lenient),
		Opt::Encode(Encode::Params { params, lenient }) =>
			encode_params(&params, lenient),
		Opt::Decode(Decode::Function { abi_path, function_name_or_signature, data }) =>
			decode_call_output(&abi_path, &function_name_or_signature, &data),
		Opt::Decode(Decode::Params { types, data }) =>
			decode_params(&types, &data),
		Opt::Decode(Decode::Log { abi_path, event_name_or_signature, topics, data }) =>
			decode_log(&abi_path, &event_name_or_signature, &topics, &data),
	}
}

fn load_function(path: &str, name_or_signature: &str) -> Result<Function, Error> {
	let file = File::open(path)?;
	let contract = Contract::load(file)?;
	let params_start = name_or_signature.find('(');

	match params_start {
		// It's a signature
		Some(params_start) => {
			let name = &name_or_signature[..params_start];

			contract.functions_by_name(name)?.iter().find(|f| {
				f.signature() == name_or_signature
			}).cloned().ok_or(Error::InvalidFunctionSignature(name_or_signature.to_owned()))
		},

		// It's a name
		None => {
			let functions = contract.functions_by_name(name_or_signature)?;
			match functions.len() {
				0 => unreachable!(),
				1 => Ok(functions[0].clone()),
				_ => Err(Error::AmbiguousFunctionName(name_or_signature.to_owned()))
			}
		},
	}
}

fn load_event(path: &str, name_or_signature: &str) -> Result<Event, Error> {
	let file = File::open(path)?;
	let contract = Contract::load(file)?;
	let params_start = name_or_signature.find('(');

	match params_start {
		// It's a signature.
		Some(params_start) => {
			let name = &name_or_signature[..params_start];
			let signature = hash_signature(name_or_signature);
			contract.events_by_name(name)?.iter().find(|event|
				event.signature() == signature
			).cloned().ok_or(Error::InvalidSignature(signature))
		}

		// It's a name.
		None => {
			let events = contract.events_by_name(name_or_signature)?;
			match events.len() {
				0 => unreachable!(),
				1 => Ok(events[0].clone()),
				_ => Err(Error::AmbiguousEventName(name_or_signature.to_string()))
			}
		}
	}
}

fn parse_tokens(params: &[(ParamType, &str)], lenient: bool) -> Result<Vec<Token>, Error> {
	params.iter()
		.map(|&(ref param, value)| match lenient {
			true => LenientTokenizer::tokenize(param, value),
			false => StrictTokenizer::tokenize(param, value)
		})
		.collect::<Result<_, _>>()
		.map_err(From::from)
}

fn encode_input(path: &str, name_or_signature: &str, values: &[String], lenient: bool) -> Result<String, Error> {
	let function = load_function(path, name_or_signature)?;

	let params: Vec<_> = function.inputs.iter()
		.map(|param| param.kind.clone())
		.zip(values.iter().map(|v| v as &str))
		.collect();

	let tokens = parse_tokens(&params, lenient)?;
	let result = function.encode_input(&tokens)?;

	Ok(result.to_hex())
}

fn encode_params(params: &[String], lenient: bool) -> Result<String, Error> {
	assert_eq!(params.len() % 2, 0);

	let params = params
		.iter()
		.tuples::<(_, _)>()
		.map(|(x, y)| Reader::read(x).map(|z| (z, y.as_str())))
		.collect::<Result<Vec<_>, _>>()?;

	let tokens = parse_tokens(params.as_slice(), lenient)?;
	let result = encode(&tokens);

	Ok(result.to_hex())
}

fn decode_call_output(path: &str, name_or_signature: &str, data: &str) -> Result<String, Error> {
	let function = load_function(path, name_or_signature)?;
	let data: Vec<u8> = data.from_hex()?;
	let tokens = function.decode_output(&data)?;
	let types = function.outputs;

	assert_eq!(types.len(), tokens.len());

	let result = types.iter()
		.zip(tokens.iter())
		.map(|(ty, to)| format!("{} {}", ty.kind, to))
		.collect::<Vec<String>>()
		.join("\n");

	Ok(result)
}

fn decode_params(types: &[String], data: &str) -> Result<String, Error> {
	let types: Vec<ParamType> = types.iter()
		.map(|s| Reader::read(s))
		.collect::<Result<_, _>>()?;

	let data: Vec<u8> = data.from_hex()?;

	let tokens = decode(&types, &data)?;

	assert_eq!(types.len(), tokens.len());

	let result = types.iter()
		.zip(tokens.iter())
		.map(|(ty, to)| format!("{} {}", ty, to))
		.collect::<Vec<String>>()
		.join("\n");

	Ok(result)
}

fn decode_log(path: &str, name_or_signature: &str, topics: &[String], data: &str) -> Result<String, Error> {
	let event = load_event(path, name_or_signature)?;
	let topics: Vec<Hash> = topics.into_iter()
		.map(|t| t.parse() )
		.collect::<Result<_, _>>()?;
	let data = data.from_hex()?;
	let decoded = event.parse_log((topics, data).into())?;

	let result = decoded.params.into_iter()
		.map(|log_param| format!("{} {}", log_param.name, log_param.value))
		.collect::<Vec<String>>()
		.join("\n");

	Ok(result)
}

fn hash_signature(sig: &str) -> Hash {
    let mut result = [0u8; 32];
    let data = sig.replace(" ", "").into_bytes();
    let mut sponge = Keccak::new_keccak256();
    sponge.update(&data);
    sponge.finalize(&mut result);
    Hash::from_slice(&result)
}

#[cfg(test)]
mod tests {
	use super::execute;

	#[test]
	fn simple_encode() {
		let command = "ethabi encode params -v bool 1".split(" ");
		let expected = "0000000000000000000000000000000000000000000000000000000000000001";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn int_encode() {
		let command = "ethabi encode params -v int256 -2 --lenient".split(" ");
		let expected = "fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn uint_encode_must_be_positive() {
		let command = "ethabi encode params -v uint256 -2 --lenient".split(" ");
		assert!(execute(command).is_err());
	}

	#[test]
	fn multi_encode() {
		let command = "ethabi encode params -v bool 1 -v string gavofyork -v bool 0".split(" ");
		let expected = "00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000096761766f66796f726b0000000000000000000000000000000000000000000000";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn array_encode() {
		let command = "ethabi encode params -v bool[] [1,0,false]".split(" ");
		let expected = "00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn function_encode_by_name() {
		let command = "ethabi encode function ../res/test.abi foo -p 1".split(" ");
		let expected = "455575780000000000000000000000000000000000000000000000000000000000000001";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn function_encode_by_signature() {
		let command = "ethabi encode function ../res/test.abi foo(bool) -p 1".split(" ");
		let expected = "455575780000000000000000000000000000000000000000000000000000000000000001";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn nonexistent_function() {
		// This should fail because there is no function called 'nope' in the ABI
		let command = "ethabi encode function ../res/test.abi nope -p 1".split(" ");
		assert!(execute(command).is_err());
	}

	#[test]
	fn overloaded_function_encode_by_name() {
		// This should fail because there are two definitions of `bar in the ABI
		let command = "ethabi encode function ../res/test.abi bar -p 1".split(" ");
		assert!(execute(command).is_err());
	}

	#[test]
	fn overloaded_function_encode_by_first_signature() {
		let command = "ethabi encode function ../res/test.abi bar(bool) -p 1".split(" ");
		let expected = "6fae94120000000000000000000000000000000000000000000000000000000000000001";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn overloaded_function_encode_by_second_signature() {
		let command = "ethabi encode function ../res/test.abi bar(string):(uint256) -p 1".split(" ");
		let expected = "d473a8ed0000000000000000000000000000000000000000000000000000000000000020\
						000000000000000000000000000000000000000000000000000000000000000131000000\
						00000000000000000000000000000000000000000000000000000000";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn simple_decode() {
		let command = "ethabi decode params -t bool 0000000000000000000000000000000000000000000000000000000000000001".split(" ");
		let expected = "bool true";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn int_decode() {
		let command = "ethabi decode params -t int256 fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe".split(" ");
		let expected = "int256 fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn multi_decode() {
		let command = "ethabi decode params -t bool -t string -t bool 00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000096761766f66796f726b0000000000000000000000000000000000000000000000".split(" ");
		let expected =
"bool true
string gavofyork
bool false";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn array_decode() {
		let command = "ethabi decode params -t bool[] 00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".split(" ");
		let expected = "bool[] [true,false,false]";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn abi_decode() {
		let command = "ethabi decode function ../res/foo.abi bar 0000000000000000000000000000000000000000000000000000000000000001".split(" ");
		let expected = "bool true";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn log_decode() {
		let command = "ethabi decode log ../res/event.abi Event -l 0000000000000000000000000000000000000000000000000000000000000001 0000000000000000000000004444444444444444444444444444444444444444".split(" ");
		let expected =
"a true
b 4444444444444444444444444444444444444444";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn log_decode_signature() {
		let command = "ethabi decode log ../res/event.abi Event(bool,address) -l 0000000000000000000000000000000000000000000000000000000000000001 0000000000000000000000004444444444444444444444444444444444444444".split(" ");
		let expected =
"a true
b 4444444444444444444444444444444444444444";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn nonexistent_event() {
		// This should return an error because no event 'Nope(bool,address)' exists
		let command = "ethabi decode log ../res/event.abi Nope(bool,address) -l 0000000000000000000000000000000000000000000000000000000000000000 0000000000000000000000004444444444444444444444444444444444444444".split(" ");
		assert!(execute(command).is_err());
	}
}
