//! ABI encoder.

use util::pad_u32;
use {Word, Token, Bytes};

fn pad_bytes(bytes: &[u8]) -> Vec<Word> {
	let mut result = vec![pad_u32(bytes.len() as u32)];
	result.extend(pad_fixed_bytes(bytes));
	result
}

fn pad_fixed_bytes(bytes: &[u8]) -> Vec<Word> {
	let mut result = vec![];
	let len = (bytes.len() + 31) / 32;
	for i in 0..len {
		let mut padded = [0u8; 32];

		let to_copy = match i == len - 1 {
			false => 32,
			true => match bytes.len() % 32 {
				0 => 32,
				x => x,
			},
		};

		let offset = 32 * i;
		padded[..to_copy].copy_from_slice(&bytes[offset..offset + to_copy]);
		result.push(padded);
	}

	result
}

#[derive(Debug)]
enum Mediate {
	Raw(Vec<Word>),
	Prefixed(Vec<Word>),
	FixedArray(Vec<Mediate>),
	Array(Vec<Mediate>),
}

impl Mediate {
	fn init_len(&self) -> u32 {
		match *self {
			Mediate::Raw(ref raw) => 32 * raw.len() as u32,
			Mediate::Prefixed(_) => 32,
			Mediate::FixedArray(ref nes) => nes.iter().fold(0, |acc, m| acc + m.init_len()),
			Mediate::Array(_) => 32,
		}
	}

	fn closing_len(&self) -> u32 {
		match *self {
			Mediate::Raw(_) => 0,
			Mediate::Prefixed(ref pre) => pre.len() as u32 * 32,
			Mediate::FixedArray(ref nes) => nes.iter().fold(0, |acc, m| acc + m.closing_len()),
			Mediate::Array(ref nes) => nes.iter().fold(32, |acc, m| acc + m.init_len() + m.closing_len()),
		}
	}

	fn offset_for(mediates: &[Mediate], position: usize) -> u32 {
		assert!(position < mediates.len());

		let init_len = mediates.iter().fold(0, |acc, m| acc + m.init_len());
		mediates[0..position].iter().fold(init_len, |acc, m| acc + m.closing_len())
	}

	fn init(&self, suffix_offset: u32) -> Vec<Word> {
		match *self {
			Mediate::Raw(ref raw) => raw.clone(),
			Mediate::FixedArray(ref nes) => {
				nes.iter()
					.enumerate()
					.flat_map(|(i, m)| m.init(Mediate::offset_for(nes, i)))
					.collect()
			},
			Mediate::Prefixed(_) | Mediate::Array(_) => {
				vec![pad_u32(suffix_offset)]
			}
		}
	}

	fn closing(&self, offset: u32) -> Vec<Word> {
		match *self {
			Mediate::Raw(_) => vec![],
			Mediate::Prefixed(ref pre) => pre.clone(),
			Mediate::FixedArray(ref nes) => {
				// offset is not taken into account, cause it would be counted twice
				// fixed array is just raw representations of similar consecutive items
				nes.iter()
					.enumerate()
					.flat_map(|(i, m)| m.closing(Mediate::offset_for(nes, i)))
					.collect()
			},
			Mediate::Array(ref nes) => {
				// + 32 added to offset represents len of the array prepanded to closing
				let prefix = vec![pad_u32(nes.len() as u32)].into_iter();

				let inits = nes.iter()
					.enumerate()
					.flat_map(|(i, m)| m.init(Mediate::offset_for(nes, i)));

				let closings = nes.iter()
					.enumerate()
					.flat_map(|(i, m)| m.closing(offset + Mediate::offset_for(nes, i)));

				prefix.chain(inits).chain(closings).collect()
			},
		}
	}
}

/// Encodes vector of tokens into ABI compliant vector of bytes.
pub fn encode(tokens: &[Token]) -> Bytes {
	let mediates: Vec<Mediate> = tokens.iter()
		.map(encode_token)
		.collect();

	let inits = mediates.iter()
		.enumerate()
		.flat_map(|(i, m)| m.init(Mediate::offset_for(&mediates, i)));

	let closings = mediates.iter()
		.enumerate()
		.flat_map(|(i, m)| m.closing(Mediate::offset_for(&mediates, i)));

	inits.chain(closings)
		.flat_map(|item| item.to_vec())
		.collect()
}

fn encode_token(token: &Token) -> Mediate {
	match *token {
		Token::Address(ref address) => {
			let mut padded = [0u8; 32];
			padded[12..].copy_from_slice(address.as_ref());
			Mediate::Raw(vec![padded])
		},
		Token::Bytes(ref bytes) => Mediate::Prefixed(pad_bytes(bytes)),
		Token::String(ref s) => Mediate::Prefixed(pad_bytes(s.as_bytes())),
		Token::FixedBytes(ref bytes) => Mediate::Raw(pad_fixed_bytes(bytes)),
		Token::Int(int) => Mediate::Raw(vec![int.into()]),
		Token::Uint(uint) => Mediate::Raw(vec![uint.into()]),
		Token::Bool(b) => {
			let mut value = [0u8; 32];
			if b {
				value[31] = 1;
			}
			Mediate::Raw(vec![value])
		},
		Token::Array(ref tokens) => {
			let mediates = tokens.iter()
				.map(encode_token)
				.collect();

			Mediate::Array(mediates)
		},
		Token::FixedArray(ref tokens) => {
			let mediates = tokens.iter()
				.map(encode_token)
				.collect();

			Mediate::FixedArray(mediates)
		},
	}
}

#[cfg(test)]
mod tests {
	use util::pad_u32;
	use {Token, encode};

	#[test]
	fn encode_address() {
		let address = Token::Address([0x11u8; 20].into());
		let encoded = encode(&vec![address]);
		let expected = hex!("0000000000000000000000001111111111111111111111111111111111111111");
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_dynamic_array_of_addresses() {
		let address1 = Token::Address([0x11u8; 20].into());
		let address2 = Token::Address([0x22u8; 20].into());
		let addresses = Token::Array(vec![address1, address2]);
		let encoded = encode(&vec![addresses]);
		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000020
			0000000000000000000000000000000000000000000000000000000000000002
			0000000000000000000000001111111111111111111111111111111111111111
			0000000000000000000000002222222222222222222222222222222222222222
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_fixed_array_of_addresses() {
		let address1 = Token::Address([0x11u8; 20].into());
		let address2 = Token::Address([0x22u8; 20].into());
		let addresses = Token::FixedArray(vec![address1, address2]);
		let encoded = encode(&vec![addresses]);
		let expected = hex!("
			0000000000000000000000001111111111111111111111111111111111111111
			0000000000000000000000002222222222222222222222222222222222222222
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_two_addresses() {
		let address1 = Token::Address([0x11u8; 20].into());
		let address2 = Token::Address([0x22u8; 20].into());
		let encoded = encode(&vec![address1, address2]);
		let expected = hex!("
			0000000000000000000000001111111111111111111111111111111111111111
			0000000000000000000000002222222222222222222222222222222222222222
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_fixed_array_of_dynamic_array_of_addresses() {
		let address1 = Token::Address([0x11u8; 20].into());
		let address2 = Token::Address([0x22u8; 20].into());
		let address3 = Token::Address([0x33u8; 20].into());
		let address4 = Token::Address([0x44u8; 20].into());
		let array0 = Token::Array(vec![address1, address2]);
		let array1 = Token::Array(vec![address3, address4]);
		let fixed = Token::FixedArray(vec![array0, array1]);
		let encoded = encode(&vec![fixed]);
		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000040
			00000000000000000000000000000000000000000000000000000000000000a0
			0000000000000000000000000000000000000000000000000000000000000002
			0000000000000000000000001111111111111111111111111111111111111111
			0000000000000000000000002222222222222222222222222222222222222222
			0000000000000000000000000000000000000000000000000000000000000002
			0000000000000000000000003333333333333333333333333333333333333333
			0000000000000000000000004444444444444444444444444444444444444444
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_dynamic_array_of_fixed_array_of_addresses() {
		let address1 = Token::Address([0x11u8; 20].into());
		let address2 = Token::Address([0x22u8; 20].into());
		let address3 = Token::Address([0x33u8; 20].into());
		let address4 = Token::Address([0x44u8; 20].into());
		let array0 = Token::FixedArray(vec![address1, address2]);
		let array1 = Token::FixedArray(vec![address3, address4]);
		let dynamic = Token::Array(vec![array0, array1]);
		let encoded = encode(&vec![dynamic]);
		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000020
			0000000000000000000000000000000000000000000000000000000000000002
			0000000000000000000000001111111111111111111111111111111111111111
			0000000000000000000000002222222222222222222222222222222222222222
			0000000000000000000000003333333333333333333333333333333333333333
			0000000000000000000000004444444444444444444444444444444444444444
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_dynamic_array_of_dynamic_arrays() {
		let address1 = Token::Address([0x11u8; 20].into());
		let address2 = Token::Address([0x22u8; 20].into());
		let array0 = Token::Array(vec![address1]);
		let array1 = Token::Array(vec![address2]);
		let dynamic = Token::Array(vec![array0, array1]);
		let encoded = encode(&vec![dynamic]);
		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000020
			0000000000000000000000000000000000000000000000000000000000000002
			0000000000000000000000000000000000000000000000000000000000000040
			0000000000000000000000000000000000000000000000000000000000000080
			0000000000000000000000000000000000000000000000000000000000000001
			0000000000000000000000001111111111111111111111111111111111111111
			0000000000000000000000000000000000000000000000000000000000000001
			0000000000000000000000002222222222222222222222222222222222222222
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_dynamic_array_of_dynamic_arrays2() {
		let address1 = Token::Address([0x11u8; 20].into());
		let address2 = Token::Address([0x22u8; 20].into());
		let address3 = Token::Address([0x33u8; 20].into());
		let address4 = Token::Address([0x44u8; 20].into());
		let array0 = Token::Array(vec![address1, address2]);
		let array1 = Token::Array(vec![address3, address4]);
		let dynamic = Token::Array(vec![array0, array1]);
		let encoded = encode(&vec![dynamic]);
		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000020
			0000000000000000000000000000000000000000000000000000000000000002
			0000000000000000000000000000000000000000000000000000000000000040
			00000000000000000000000000000000000000000000000000000000000000a0
			0000000000000000000000000000000000000000000000000000000000000002
			0000000000000000000000001111111111111111111111111111111111111111
			0000000000000000000000002222222222222222222222222222222222222222
			0000000000000000000000000000000000000000000000000000000000000002
			0000000000000000000000003333333333333333333333333333333333333333
			0000000000000000000000004444444444444444444444444444444444444444
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_fixed_array_of_fixed_arrays() {
		let address1 = Token::Address([0x11u8; 20].into());
		let address2 = Token::Address([0x22u8; 20].into());
		let address3 = Token::Address([0x33u8; 20].into());
		let address4 = Token::Address([0x44u8; 20].into());
		let array0 = Token::FixedArray(vec![address1, address2]);
		let array1 = Token::FixedArray(vec![address3, address4]);
		let fixed = Token::FixedArray(vec![array0, array1]);
		let encoded = encode(&vec![fixed]);
		let expected = hex!("
			0000000000000000000000001111111111111111111111111111111111111111
			0000000000000000000000002222222222222222222222222222222222222222
			0000000000000000000000003333333333333333333333333333333333333333
			0000000000000000000000004444444444444444444444444444444444444444
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_empty_array() {
		// Empty arrays
		let encoded = encode(&vec![
			Token::Array(vec![]),
			Token::Array(vec![])]
		);
		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000040
			0000000000000000000000000000000000000000000000000000000000000060
			0000000000000000000000000000000000000000000000000000000000000000
			0000000000000000000000000000000000000000000000000000000000000000
		").to_vec();
		assert_eq!(encoded, expected);

		// Nested empty arrays
		let encoded = encode(&vec![
			Token::Array(vec![Token::Array(vec![])]),
			Token::Array(vec![Token::Array(vec![])]),
		]);
		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000040
			00000000000000000000000000000000000000000000000000000000000000a0
			0000000000000000000000000000000000000000000000000000000000000001
			0000000000000000000000000000000000000000000000000000000000000020
			0000000000000000000000000000000000000000000000000000000000000000
			0000000000000000000000000000000000000000000000000000000000000001
			0000000000000000000000000000000000000000000000000000000000000020
			0000000000000000000000000000000000000000000000000000000000000000
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_bytes() {
		let bytes = Token::Bytes(vec![0x12, 0x34]);
		let encoded = encode(&vec![bytes]);
		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000020
			0000000000000000000000000000000000000000000000000000000000000002
			1234000000000000000000000000000000000000000000000000000000000000
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_fixed_bytes() {
		let bytes = Token::FixedBytes(vec![0x12, 0x34]);
		let encoded = encode(&vec![bytes]);
		let expected = hex!("1234000000000000000000000000000000000000000000000000000000000000");
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_string() {
		let s = Token::String("gavofyork".to_owned());
		let encoded = encode(&vec![s]);
		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000020
			0000000000000000000000000000000000000000000000000000000000000009
			6761766f66796f726b0000000000000000000000000000000000000000000000
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_bytes2() {
		let bytes = Token::Bytes(hex!("10000000000000000000000000000000000000000000000000000000000002").to_vec());
		let encoded = encode(&vec![bytes]);
		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000020
			000000000000000000000000000000000000000000000000000000000000001f
			1000000000000000000000000000000000000000000000000000000000000200
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_bytes3() {
		let bytes = Token::Bytes(hex!("
			1000000000000000000000000000000000000000000000000000000000000000
			1000000000000000000000000000000000000000000000000000000000000000
		").to_vec());
		let encoded = encode(&vec![bytes]);
		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000020
			0000000000000000000000000000000000000000000000000000000000000040
			1000000000000000000000000000000000000000000000000000000000000000
			1000000000000000000000000000000000000000000000000000000000000000
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_two_bytes() {
		let bytes1 = Token::Bytes(hex!("10000000000000000000000000000000000000000000000000000000000002").to_vec());
		let bytes2 = Token::Bytes(hex!("0010000000000000000000000000000000000000000000000000000000000002").to_vec());
		let encoded = encode(&vec![bytes1, bytes2]);
		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000040
			0000000000000000000000000000000000000000000000000000000000000080
			000000000000000000000000000000000000000000000000000000000000001f
			1000000000000000000000000000000000000000000000000000000000000200
			0000000000000000000000000000000000000000000000000000000000000020
			0010000000000000000000000000000000000000000000000000000000000002
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_uint() {
		let mut uint = [0u8; 32];
		uint[31] = 4;
		let encoded = encode(&vec![Token::Uint(uint.into())]);
		let expected = hex!("0000000000000000000000000000000000000000000000000000000000000004");
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_int() {
		let mut int = [0u8; 32];
		int[31] = 4;
		let encoded = encode(&vec![Token::Int(int.into())]);
		let expected = hex!("0000000000000000000000000000000000000000000000000000000000000004");
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_bool() {
		let encoded = encode(&vec![Token::Bool(true)]);
		let expected = hex!("0000000000000000000000000000000000000000000000000000000000000001");
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_bool2() {
		let encoded = encode(&vec![Token::Bool(false)]);
		let expected = hex!("0000000000000000000000000000000000000000000000000000000000000000");
		assert_eq!(encoded, expected);
	}

	#[test]
	fn comprehensive_test() {
		let bytes = hex!("
			131a3afc00d1b1e3461b955e53fc866dcf303b3eb9f4c16f89e388930f48134b
			131a3afc00d1b1e3461b955e53fc866dcf303b3eb9f4c16f89e388930f48134b
		").to_vec();
		let encoded = encode(&vec![
			Token::Int(5.into()),
			Token::Bytes(bytes.clone()),
			Token::Int(3.into()),
			Token::Bytes(bytes)
		]);

		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000005
			0000000000000000000000000000000000000000000000000000000000000080
			0000000000000000000000000000000000000000000000000000000000000003
			00000000000000000000000000000000000000000000000000000000000000e0
			0000000000000000000000000000000000000000000000000000000000000040
			131a3afc00d1b1e3461b955e53fc866dcf303b3eb9f4c16f89e388930f48134b
			131a3afc00d1b1e3461b955e53fc866dcf303b3eb9f4c16f89e388930f48134b
			0000000000000000000000000000000000000000000000000000000000000040
			131a3afc00d1b1e3461b955e53fc866dcf303b3eb9f4c16f89e388930f48134b
			131a3afc00d1b1e3461b955e53fc866dcf303b3eb9f4c16f89e388930f48134b
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn test_pad_u32() {
		// this will fail if endianess is not supported
		assert_eq!(pad_u32(0x1)[31], 1);
		assert_eq!(pad_u32(0x100)[30], 1);
	}

	#[test]
	fn comprehensive_test2() {
		let encoded = encode(&vec![
			Token::Int(1.into()),
			Token::String("gavofyork".to_owned()),
			Token::Int(2.into()),
			Token::Int(3.into()),
			Token::Int(4.into()),
			Token::Array(vec![
				Token::Int(5.into()),
				Token::Int(6.into()),
				Token::Int(7.into()),
			])
		]);

		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000001
			00000000000000000000000000000000000000000000000000000000000000c0
			0000000000000000000000000000000000000000000000000000000000000002
			0000000000000000000000000000000000000000000000000000000000000003
			0000000000000000000000000000000000000000000000000000000000000004
			0000000000000000000000000000000000000000000000000000000000000100
			0000000000000000000000000000000000000000000000000000000000000009
			6761766f66796f726b0000000000000000000000000000000000000000000000
			0000000000000000000000000000000000000000000000000000000000000003
			0000000000000000000000000000000000000000000000000000000000000005
			0000000000000000000000000000000000000000000000000000000000000006
			0000000000000000000000000000000000000000000000000000000000000007
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_dynamic_array_of_bytes() {
		let bytes =
			hex!("019c80031b20d5e69c8093a571162299032018d913930d93ab320ae5ea44a4218a274f00d607");
		let encoded = encode(&vec![Token::Array(vec![Token::Bytes(bytes.to_vec())])]);

		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000020
			0000000000000000000000000000000000000000000000000000000000000001
			0000000000000000000000000000000000000000000000000000000000000020
			0000000000000000000000000000000000000000000000000000000000000026
			019c80031b20d5e69c8093a571162299032018d913930d93ab320ae5ea44a421
			8a274f00d6070000000000000000000000000000000000000000000000000000
		").to_vec();
		assert_eq!(encoded, expected);
	}

	#[test]
	fn encode_dynamic_array_of_bytes2() {
		let bytes =
			hex!("4444444444444444444444444444444444444444444444444444444444444444444444444444");
		let bytes2 =
			hex!("6666666666666666666666666666666666666666666666666666666666666666666666666666");
		let encoded = encode(&vec![Token::Array(vec![
			Token::Bytes(bytes.to_vec()),
			Token::Bytes(bytes2.to_vec()),
		])]);

		let expected = hex!("
			0000000000000000000000000000000000000000000000000000000000000020
			0000000000000000000000000000000000000000000000000000000000000002
			0000000000000000000000000000000000000000000000000000000000000040
			00000000000000000000000000000000000000000000000000000000000000a0
			0000000000000000000000000000000000000000000000000000000000000026
			4444444444444444444444444444444444444444444444444444444444444444
			4444444444440000000000000000000000000000000000000000000000000000
			0000000000000000000000000000000000000000000000000000000000000026
			6666666666666666666666666666666666666666666666666666666666666666
			6666666666660000000000000000000000000000000000000000000000000000
		").to_vec();
		assert_eq!(encoded, expected);
	}
}

