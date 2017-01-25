// Copyright 2015-2017Parity Technologies (UK) Ltd.
// This file is part of Parity.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

/// Validator set maintained in a contract, updated using `getValidators` method.
/// It can also report validators for misbehaviour with two levels: `reportMalicious` and `reportBenign`.

use std::sync::Weak;
use util::*;
use client::{Client, BlockChainClient};
use super::ValidatorSet;
use super::safe_contract::ValidatorSafeContract;

/// The validator contract should have the following interface:
/// [{"constant":true,"inputs":[],"name":"getValidators","outputs":[{"name":"","type":"address[]"}],"payable":false,"type":"function"}]
pub struct ValidatorContract {
	validators: Arc<ValidatorSafeContract>,
	provider: RwLock<Option<provider::Contract>>,
}

impl ValidatorContract {
	pub fn new(contract_address: Address) -> Self {
		ValidatorContract {
			validators: Arc::new(ValidatorSafeContract::new(contract_address)),
			provider: RwLock::new(None),
		}
	}
}

impl ValidatorSet for Arc<ValidatorContract> {
	fn contains(&self, address: &Address) -> bool {
		self.validators.contains(address)
	}

	fn get(&self, nonce: usize) -> Address {
		self.validators.get(nonce)
	}

	fn count(&self) -> usize {
		self.validators.count()
	}

	fn report_malicious(&self, address: &Address) {
		if let Some(ref provider) = *self.provider.read() {
			match provider.report_malicious(address) {
				Ok(_) => warn!(target: "engine", "Reported malicious validator {}", address),
				Err(s) => warn!(target: "engine", "Validator {} could not be reported {}", address, s),
			}
		} else {
			warn!(target: "engine", "Malicious behaviour could not be reported: no provider contract.")
		}
	}

	fn report_benign(&self, address: &Address) {
		if let Some(ref provider) = *self.provider.read() {
			match provider.report_benign(address) {
				Ok(_) => warn!(target: "engine", "Reported benign validator misbehaviour {}", address),
				Err(s) => warn!(target: "engine", "Validator {} could not be reported {}", address, s),
			}
		} else {
			warn!(target: "engine", "Benign misbehaviour could not be reported: no provider contract.")
		}
	}

	fn register_contract(&self, client: Weak<Client>) {
		self.validators.register_contract(client.clone());
		let transact = move |a, d| client
			.upgrade()
			.ok_or("No client!".into())
			.and_then(|c| c.transact_contract(a, d).map_err(|e| format!("Transaction import error: {}", e)))
			.map(|_| Default::default());
		*self.provider.write() = Some(provider::Contract::new(self.validators.address, transact));
	}
}

mod provider {
	// Autogenerated from JSON contract definition using Rust contract convertor.
	#![allow(unused_imports)]
	use std::string::String;
	use std::result::Result;
	use std::fmt;
	use {util, ethabi};
	use util::{FixedHash, Uint};

	pub struct Contract {
		contract: ethabi::Contract,
		address: util::Address,
		do_call: Box<Fn(util::Address, Vec<u8>) -> Result<Vec<u8>, String> + Send + Sync + 'static>,
	}
	impl Contract {
		pub fn new<F>(address: util::Address, do_call: F) -> Self where F: Fn(util::Address, Vec<u8>) -> Result<Vec<u8>, String> + Send + Sync + 'static {
			Contract {
				contract: ethabi::Contract::new(ethabi::Interface::load(b"[{\"constant\":false,\"inputs\":[{\"name\":\"validator\",\"type\":\"address\"}],\"name\":\"reportMalicious\",\"outputs\":[],\"payable\":false,\"type\":\"function\"},{\"constant\":false,\"inputs\":[{\"name\":\"validator\",\"type\":\"address\"}],\"name\":\"reportBenign\",\"outputs\":[],\"payable\":false,\"type\":\"function\"}]").expect("JSON is autogenerated; qed")),
				address: address,
				do_call: Box::new(do_call),
			}
		}
		fn as_string<T: fmt::Debug>(e: T) -> String { format!("{:?}", e) }
		
		/// Auto-generated from: `{"constant":false,"inputs":[{"name":"validator","type":"address"}],"name":"reportMalicious","outputs":[],"payable":false,"type":"function"}`
		#[allow(dead_code)]
		pub fn report_malicious(&self, validator: &util::Address) -> Result<(), String> {
			let call = self.contract.function("reportMalicious".into()).map_err(Self::as_string)?;
			let data = call.encode_call(
				vec![ethabi::Token::Address(validator.clone().0)]
			).map_err(Self::as_string)?;
			call.decode_output((self.do_call)(self.address.clone(), data)?).map_err(Self::as_string)?;
			
			Ok(())
		}

		/// Auto-generated from: `{"constant":false,"inputs":[{"name":"validator","type":"address"}],"name":"reportBenign","outputs":[],"payable":false,"type":"function"}`
		#[allow(dead_code)]
		pub fn report_benign(&self, validator: &util::Address) -> Result<(), String> {
			let call = self.contract.function("reportBenign".into()).map_err(Self::as_string)?;
			let data = call.encode_call(
				vec![ethabi::Token::Address(validator.clone().0)]
			).map_err(Self::as_string)?;
			call.decode_output((self.do_call)(self.address.clone(), data)?).map_err(Self::as_string)?;
			
			Ok(())
		}
	}
}

#[cfg(test)]
mod tests {
	use util::*;
	use rlp::encode;
	use ethkey::Secret;
	use spec::Spec;
	use header::Header;
	use account_provider::AccountProvider;
	use miner::MinerService;
	use client::BlockChainClient;
	use tests::helpers::generate_dummy_client_with_spec_and_accounts;
	use super::super::ValidatorSet;
	use super::ValidatorContract;

	#[test]
	fn fetches_validators() {
		let client = generate_dummy_client_with_spec_and_accounts(Spec::new_validator_contract, None);
		let vc = Arc::new(ValidatorContract::new(Address::from_str("0000000000000000000000000000000000000005").unwrap()));
		vc.register_contract(Arc::downgrade(&client));
		assert!(vc.contains(&Address::from_str("7d577a597b2742b498cb5cf0c26cdcd726d39e6e").unwrap()));
		assert!(vc.contains(&Address::from_str("82a978b3f5962a5b0957d9ee9eef472ee55b42f1").unwrap()));
	}
	
	#[test]
	fn reports_validators() {
		let tap = Arc::new(AccountProvider::transient_provider());
		let v1 = tap.insert_account(Secret::from_slice(&"1".sha3()).unwrap(), "").unwrap();
		let client = generate_dummy_client_with_spec_and_accounts(Spec::new_validator_contract, Some(tap.clone()));
		client.engine().register_client(Arc::downgrade(&client));
		let validator_contract = Address::from_str("0000000000000000000000000000000000000005").unwrap();

		// Make sure reporting can be done.
		client.miner().set_gas_floor_target(1_000_000.into());

		client.miner().set_engine_signer(v1, "".into()).unwrap();
		let mut header = Header::default();
		let seal = encode(&vec!(5u8)).to_vec();	
		header.set_seal(vec!(seal));
		header.set_author(v1);
		header.set_number(1);
		// `reportBenign` when the designated proposer releases block from the future (bad clock).
		assert!(client.engine().verify_block_unordered(&header, None).is_err());
		// Seal a block.
		client.engine().step();
		assert_eq!(client.chain_info().best_block_number, 1);
		// Check if the unresponsive validator is `disliked`.
		assert_eq!(client.call_contract(validator_contract, "d8f2e0bf".from_hex().unwrap()).unwrap().to_hex(), "0000000000000000000000007d577a597b2742b498cb5cf0c26cdcd726d39e6e");
		// Simulate a misbehaving validator by handling a double proposal.
		assert!(client.engine().verify_block_family(&header, &header, None).is_err());
		// Seal a block.
		client.engine().step();
		client.engine().step();
		assert_eq!(client.chain_info().best_block_number, 2);

		// Check if misbehaving validator was removed.
		client.transact_contract(Default::default(), Default::default()).unwrap();
		client.engine().step();
		client.engine().step();
		assert_eq!(client.chain_info().best_block_number, 2);
	}
}
