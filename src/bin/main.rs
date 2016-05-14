extern crate docopt;
extern crate rustc_serialize;
extern crate ethabi;

mod error;

use std::fs::File;
use std::io::Read;
use std::env;
use docopt::Docopt;
use rustc_serialize::hex::{ToHex, FromHex};
use ethabi::spec::Interface;
use ethabi::spec::param_type::{ParamType, Reader}; 
use ethabi::token::{Token, Tokenizer, StrictTokenizer, TokenFromHex};
use ethabi::{Encoder, Decoder, Contract, Function, Event};
use error::Error;

pub const ETHABI: &'static str = r#"
Ethereum ABI coder.
  Copyright 2016 Ethcore (UK) Limited

Usage:
    ethabi encode abi <abi-path> <function-name> [-p <param>]... [-l | --lenient]
    ethabi encode params [-p <type> <param>]... [-l | --lenient]
    ethabi decode abi <abi-path> <function-name> <data>
    ethabi decode params [-p <type>]... <data>
	ethabi decode log <abi-path> <event-name> [-p <topic>]... <data>
    ethabi -h | --help

Options:
    -h, --help         Display this message and exit.
    -l, --lenient      Allow short representation of input params.

Commands:
    encode             Encode ABI call.
    decode             Decode ABI call result.
    abi                Load json ABI from file.
    params             Specify types of input params inline.
"#;

#[derive(Debug, RustcDecodable)]
struct Args {
	cmd_encode: bool,
	cmd_decode: bool,
	cmd_abi: bool,
	cmd_params: bool,
	cmd_log: bool,
	arg_abi_path: String,
	arg_function_name: String,
	arg_event_name: String,
	arg_param: Vec<String>,
	arg_type: Vec<String>,
	arg_data: String,
	arg_topic: Vec<String>,
}

fn main() {
	let result = execute(env::args());

	match result {
		Ok(s) => println!("{}", s),
		Err(error) => println!("error: {:?}", error)
	}
}

fn execute<S, I>(command: I) -> Result<String, Error> where I: IntoIterator<Item=S>, S: AsRef<str> {
	let args: Args = Docopt::new(ETHABI)
		.and_then(|d| d.argv(command).decode())
		.unwrap_or_else(|e| e.exit());

	if args.cmd_encode && args.cmd_abi {
		encode_call(&args.arg_abi_path, args.arg_function_name, args.arg_param)
	} else if args.cmd_encode && args.cmd_params {
		encode_params(args.arg_type, args.arg_param)
	} else if args.cmd_decode && args.cmd_abi {
		decode_call_output(&args.arg_abi_path, args.arg_function_name, args.arg_data)
	} else if args.cmd_decode && args.cmd_params {
		decode_params(args.arg_type, args.arg_data)
	} else if args.cmd_decode && args.cmd_log {
		decode_log(&args.arg_abi_path, args.arg_event_name, args.arg_topic, args.arg_data)
	} else {
		unreachable!()
	}
}

fn load_function(path: &str, function: String) -> Result<Function, Error> {
	let file = try!(File::open(path));
	let bytes: Vec<u8> = try!(file.bytes().collect());

	let interface = try!(Interface::load(&bytes));
	let contract = Contract::new(interface);
	let function = try!(contract.function(function));
	Ok(function)
}

fn load_event(path: &str, event: String) -> Result<Event, Error> {
	let file = try!(File::open(path));
	let bytes: Vec<u8> = try!(file.bytes().collect());

	let interface = try!(Interface::load(&bytes));
	let contract = Contract::new(interface);
	let event = try!(contract.event(event));
	Ok(event)
}

fn parse_tokens(params: &[(ParamType, String)]) -> Result<Vec<Token>, Error> {
	params.iter()
		.map(|&(ref param, ref value)| StrictTokenizer::tokenize(param, value))
		.collect::<Result<_, _>>()
		.map_err(From::from)
}

fn encode_call(path: &str, function: String, values: Vec<String>) -> Result<String, Error> {
	let function = try!(load_function(path, function));
	let types = function.input_params();

	let params: Vec<_> = types.into_iter()
		.zip(values.into_iter())
		.collect();
	
	let tokens = try!(parse_tokens(&params));
	let result = try!(function.encode_call(tokens));
	
	Ok(result.to_hex())
}

fn encode_params(types: Vec<String>, values: Vec<String>) -> Result<String, Error> {
	assert_eq!(types.len(), values.len());

	let types: Result<Vec<ParamType>, _> = types.iter()
		.map(|s| Reader::read(s))
		.collect();

	let types = try!(types);

	let params: Vec<_> = types.into_iter()
		.zip(values.into_iter())
		.collect();

	let tokens = try!(parse_tokens(&params));
	let result = Encoder::encode(tokens);

	Ok(result.to_hex())
}

fn decode_call_output(path: &str, function: String, data: String) -> Result<String, Error> {
	let function = try!(load_function(path, function));
	let data = try!(data.from_hex());

	let types = function.output_params();
	let tokens = try!(function.decode_output(data));

	assert_eq!(types.len(), tokens.len());

	let result = types.iter()
		.zip(tokens.iter())
		.map(|(ty, to)| format!("{} {}", ty, to))
		.collect::<Vec<String>>()
		.join("\n");

	Ok(result)
}

fn decode_params(types: Vec<String>, data: String) -> Result<String, Error> {
	let types: Result<Vec<ParamType>, _> = types.iter()
		.map(|s| Reader::read(s))
		.collect();

	let types = try!(types);
	let data = try!(data.from_hex());

	let tokens = try!(Decoder::decode(&types, data));

	assert_eq!(types.len(), tokens.len());

	let result = types.iter()
		.zip(tokens.iter())
		.map(|(ty, to)| format!("{} {}", ty, to))
		.collect::<Vec<String>>()
		.join("\n");

	Ok(result)
}

fn decode_log(path: &str, event: String, topics: Vec<String>, data: String) -> Result<String, Error> {
	let event = try!(load_event(path, event));
	let topics: Result<Vec<[u8; 32]>, Error> = topics.into_iter()
		.map(|t| t.token_from_hex().map_err(From::from))
		.collect();
	let topics = try!(topics);
	let data = try!(data.from_hex());
	let decoded = try!(event.decode_log(topics, data));
	
	let result = decoded.params.into_iter()
		.map(|(name, value)| format!("{} {}", name, value))
		.collect::<Vec<String>>()
		.join("\n");

	Ok(result)
}

#[cfg(test)]
mod tests {
	use super::execute;
	
	#[test]
	fn simple_encode() {
		let command = "ethabi encode params -p bool 1".split(" ");
		let expected = "0000000000000000000000000000000000000000000000000000000000000001";
		assert_eq!(execute(command).unwrap(), expected);
	}

	#[test]
	fn multi_encode() {
		let command = "ethabi encode params -p bool 1 -p string gavofyork -p bool 0".split(" ");
		let expected = "00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000096761766f66796f726b0000000000000000000000000000000000000000000000";
		assert_eq!(execute(command).unwrap(), expected);
	}
}
