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

use std::collections::BTreeMap;
use serde::{Serialize, Serializer};
use ethcore::trace::{FlatTrace, LocalizedTrace as EthLocalizedTrace, trace, TraceError};
use ethcore::trace as et;
use ethcore::state_diff;
use ethcore::account_diff;
use ethcore::executed;
use ethcore::client::Executed;
use util::Uint;
use v1::types::{Bytes, H160, H256, U256};

#[derive(Debug, Serialize)]
/// A diff of some chunk of memory.
pub struct MemoryDiff {
	/// Offset into memory the change begins.
	pub off: usize,
	/// The changed data.
	pub data: Bytes,
}

impl From<et::MemoryDiff> for MemoryDiff {
	fn from(c: et::MemoryDiff) -> Self {
		MemoryDiff {
			off: c.offset,
			data: c.data.into(),
		}
	}
}

#[derive(Debug, Serialize)]
/// A diff of some storage value.
pub struct StorageDiff {
	/// Which key in storage is changed.
	pub key: U256,
	/// What the value has been changed to.
	pub val: U256,
}

impl From<et::StorageDiff> for StorageDiff {
	fn from(c: et::StorageDiff) -> Self {
		StorageDiff {
			key: c.location.into(),
			val: c.value.into(),
		}
	}
}

#[derive(Debug, Serialize)]
/// A record of an executed VM operation.
pub struct VMExecutedOperation {
	/// The total gas used.
	#[serde(rename="used")]
	pub used: u64,
	/// The stack item placed, if any.
	pub push: Vec<U256>,
	/// If altered, the memory delta.
	#[serde(rename="mem")]
	pub mem: Option<MemoryDiff>,
	/// The altered storage value, if any.
	#[serde(rename="store")]
	pub store: Option<StorageDiff>,
}

impl From<et::VMExecutedOperation> for VMExecutedOperation {
	fn from(c: et::VMExecutedOperation) -> Self {
		VMExecutedOperation {
			used: c.gas_used.low_u64(),
			push: c.stack_push.into_iter().map(Into::into).collect(),
			mem: c.mem_diff.map(Into::into),
			store: c.store_diff.map(Into::into),
		}
	}
}

#[derive(Debug, Serialize)]
/// A record of the execution of a single VM operation.
pub struct VMOperation {
	/// The program counter.
	pub pc: usize,
	/// The gas cost for this instruction.
	pub cost: u64,
	/// Information concerning the execution of the operation.
	pub ex: Option<VMExecutedOperation>,
	/// Subordinate trace of the CALL/CREATE if applicable.
	#[serde(bound="VMTrace: Serialize")]
	pub sub: Option<VMTrace>,
}

impl From<(et::VMOperation, Option<et::VMTrace>)> for VMOperation {
	fn from(c: (et::VMOperation, Option<et::VMTrace>)) -> Self {
		VMOperation {
			pc: c.0.pc,
			cost: c.0.gas_cost.low_u64(),
			ex: c.0.executed.map(Into::into),
			sub: c.1.map(Into::into),
		}
	}
}

#[derive(Debug, Serialize)]
/// A record of a full VM trace for a CALL/CREATE.
pub struct VMTrace {
	/// The code to be executed.
	pub code: Bytes,
	/// The operations executed.
	pub ops: Vec<VMOperation>,
}

impl From<et::VMTrace> for VMTrace {
	fn from(c: et::VMTrace) -> Self {
		let mut subs = c.subs.into_iter();
		let mut next_sub = subs.next();
		VMTrace {
			code: c.code.into(),
			ops: c.operations
				.into_iter()
				.enumerate()
				.map(|(i, op)| (op, {
					let have_sub = next_sub.is_some() && next_sub.as_ref().unwrap().parent_step == i;
					if have_sub {
						let r = next_sub.clone();
						next_sub = subs.next();
						r
					} else { None }
				}).into())
				.collect(),
		}
	}
}

#[derive(Debug, Serialize)]
/// Aux type for Diff::Changed.
pub struct ChangedType<T> where T: Serialize {
	from: T,
	to: T,
}

#[derive(Debug, Serialize)]
/// Serde-friendly `Diff` shadow.
pub enum Diff<T> where T: Serialize {
	#[serde(rename="=")]
	Same,
	#[serde(rename="+")]
	Born(T),
	#[serde(rename="-")]
	Died(T),
	#[serde(rename="*")]
	Changed(ChangedType<T>),
}

impl<T, U> From<account_diff::Diff<T>> for Diff<U> where T: Eq + ::ethcore_ipc::BinaryConvertable, U: Serialize + From<T> {
	fn from(c: account_diff::Diff<T>) -> Self {
		match c {
			account_diff::Diff::Same => Diff::Same,
			account_diff::Diff::Born(t) => Diff::Born(t.into()),
			account_diff::Diff::Died(t) => Diff::Died(t.into()),
			account_diff::Diff::Changed(t, u) => Diff::Changed(ChangedType{from: t.into(), to: u.into()}),
		}
	}
}

#[derive(Debug, Serialize)]
/// Serde-friendly `AccountDiff` shadow.
pub struct AccountDiff {
	pub balance: Diff<U256>,
	pub nonce: Diff<U256>,
	pub code: Diff<Bytes>,
	pub storage: BTreeMap<H256, Diff<H256>>,
}

impl From<account_diff::AccountDiff> for AccountDiff {
	fn from(c: account_diff::AccountDiff) -> Self {
		AccountDiff {
			balance: c.balance.into(),
			nonce: c.nonce.into(),
			code: c.code.into(),
			storage: c.storage.into_iter().map(|(k, v)| (k.into(), v.into())).collect(),
		}
	}
}

#[derive(Debug)]
/// Serde-friendly `StateDiff` shadow.
pub struct StateDiff(BTreeMap<H160, AccountDiff>);

impl Serialize for StateDiff {
	fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
	where S: Serializer {
		Serialize::serialize(&self.0, serializer)
	}
}

impl From<state_diff::StateDiff> for StateDiff {
	fn from(c: state_diff::StateDiff) -> Self {
		StateDiff(c.raw.into_iter().map(|(k, v)| (k.into(), v.into())).collect())
	}
}

/// Create response
#[derive(Debug, Serialize)]
pub struct Create {
	/// Sender
	from: H160,
	/// Value
	value: U256,
	/// Gas
	gas: U256,
	/// Initialization code
	init: Bytes,
}

impl From<trace::Create> for Create {
	fn from(c: trace::Create) -> Self {
		Create {
			from: c.from.into(),
			value: c.value.into(),
			gas: c.gas.into(),
			init: Bytes::new(c.init),
		}
	}
}

/// Call type.
#[derive(Debug, Serialize)]
pub enum CallType {
	/// None
	#[serde(rename="none")]
	None,
	/// Call
	#[serde(rename="call")]
	Call,
	/// Call code
	#[serde(rename="callcode")]
	CallCode,
	/// Delegate call
	#[serde(rename="delegatecall")]
	DelegateCall,
}

impl From<executed::CallType> for CallType {
	fn from(c: executed::CallType) -> Self {
		match c {
			executed::CallType::None => CallType::None,
			executed::CallType::Call => CallType::Call,
			executed::CallType::CallCode => CallType::CallCode,
			executed::CallType::DelegateCall => CallType::DelegateCall,
		}
	}
}

/// Call response
#[derive(Debug, Serialize)]
pub struct Call {
	/// Sender
	from: H160,
	/// Recipient
	to: H160,
	/// Transfered Value
	value: U256,
	/// Gas
	gas: U256,
	/// Input data
	input: Bytes,
	/// The type of the call.
	#[serde(rename="callType")]
	call_type: CallType,
}

impl From<trace::Call> for Call {
	fn from(c: trace::Call) -> Self {
		Call {
			from: c.from.into(),
			to: c.to.into(),
			value: c.value.into(),
			gas: c.gas.into(),
			input: c.input.into(),
			call_type: c.call_type.into(),
		}
	}
}

/// Suicide
#[derive(Debug, Serialize)]
pub struct Suicide {
	/// Address.
	pub address: H160,
	/// Refund address.
	#[serde(rename="refundAddress")]
	pub refund_address: H160,
	/// Balance.
	pub balance: U256,
}

impl From<trace::Suicide> for Suicide {
	fn from(s: trace::Suicide) -> Self {
		Suicide {
			address: s.address.into(),
			refund_address: s.refund_address.into(),
			balance: s.balance.into(),
		}
	}
}

/// Action
#[derive(Debug)]
pub enum Action {
	/// Call
	Call(Call),
	/// Create
	Create(Create),
	/// Suicide
	Suicide(Suicide),
}

impl From<trace::Action> for Action {
	fn from(c: trace::Action) -> Self {
		match c {
			trace::Action::Call(call) => Action::Call(call.into()),
			trace::Action::Create(create) => Action::Create(create.into()),
			trace::Action::Suicide(suicide) => Action::Suicide(suicide.into()),
		}
	}
}

/// Call Result
#[derive(Debug, Serialize)]
pub struct CallResult {
	/// Gas used
	#[serde(rename="gasUsed")]
	gas_used: U256,
	/// Output bytes
	output: Bytes,
}

impl From<trace::CallResult> for CallResult {
	fn from(c: trace::CallResult) -> Self {
		CallResult {
			gas_used: c.gas_used.into(),
			output: c.output.into(),
		}
	}
}

/// Craete Result
#[derive(Debug, Serialize)]
pub struct CreateResult {
	/// Gas used
	#[serde(rename="gasUsed")]
	gas_used: U256,
	/// Code
	code: Bytes,
	/// Assigned address
	address: H160,
}

impl From<trace::CreateResult> for CreateResult {
	fn from(c: trace::CreateResult) -> Self {
		CreateResult {
			gas_used: c.gas_used.into(),
			code: c.code.into(),
			address: c.address.into(),
		}
	}
}

/// Response
#[derive(Debug)]
pub enum Res {
	/// Call
	Call(CallResult),
	/// Create
	Create(CreateResult),
	/// Call failure
	FailedCall(TraceError),
	/// Creation failure
	FailedCreate(TraceError),
	/// None
	None,
}

impl From<trace::Res> for Res {
	fn from(t: trace::Res) -> Self {
		match t {
			trace::Res::Call(call) => Res::Call(CallResult::from(call)),
			trace::Res::Create(create) => Res::Create(CreateResult::from(create)),
			trace::Res::FailedCall(error) => Res::FailedCall(error),
			trace::Res::FailedCreate(error) => Res::FailedCreate(error),
			trace::Res::None => Res::None,
		}
	}
}

/// Trace
#[derive(Debug)]
pub struct LocalizedTrace {
	/// Action
	action: Action,
	/// Result
	result: Res,
	/// Trace address
	trace_address: Vec<usize>,
	/// Subtraces
	subtraces: usize,
	/// Transaction position
	transaction_position: usize,
	/// Transaction hash
	transaction_hash: H256,
	/// Block Number
	block_number: u64,
	/// Block Hash
	block_hash: H256,
}

impl Serialize for LocalizedTrace {
	fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
		where S: Serializer
	{
		let mut state = serializer.serialize_struct("LocalizedTrace", 9)?;
		match self.action {
			Action::Call(ref call) => {
				serializer.serialize_struct_elt(&mut state, "type", "call")?;
				serializer.serialize_struct_elt(&mut state, "action", call)?;
			},
			Action::Create(ref create) => {
				serializer.serialize_struct_elt(&mut state, "type", "create")?;
				serializer.serialize_struct_elt(&mut state, "action", create)?;
			},
			Action::Suicide(ref suicide) => {
				serializer.serialize_struct_elt(&mut state, "type", "suicide")?;
				serializer.serialize_struct_elt(&mut state, "action", suicide)?;
			},
		}

		match self.result {
			Res::Call(ref call) => serializer.serialize_struct_elt(&mut state, "result", call)?,
			Res::Create(ref create) => serializer.serialize_struct_elt(&mut state, "result", create)?,
			Res::FailedCall(ref error) => serializer.serialize_struct_elt(&mut state, "error", error.to_string())?,
			Res::FailedCreate(ref error) => serializer.serialize_struct_elt(&mut state, "error", error.to_string())?,
			Res::None => serializer.serialize_struct_elt(&mut state, "result", None as Option<u8>)?,
		}

		serializer.serialize_struct_elt(&mut state, "traceAddress", &self.trace_address)?;
		serializer.serialize_struct_elt(&mut state, "subtraces", &self.subtraces)?;
		serializer.serialize_struct_elt(&mut state, "transactionPosition", &self.transaction_position)?;
		serializer.serialize_struct_elt(&mut state, "transactionHash", &self.transaction_hash)?;
		serializer.serialize_struct_elt(&mut state, "blockNumber", &self.block_number)?;
		serializer.serialize_struct_elt(&mut state, "blockHash", &self.block_hash)?;

		serializer.serialize_struct_end(state)
	}
}

impl From<EthLocalizedTrace> for LocalizedTrace {
	fn from(t: EthLocalizedTrace) -> Self {
		LocalizedTrace {
			action: t.action.into(),
			result: t.result.into(),
			trace_address: t.trace_address.into_iter().map(Into::into).collect(),
			subtraces: t.subtraces.into(),
			transaction_position: t.transaction_number.into(),
			transaction_hash: t.transaction_hash.into(),
			block_number: t.block_number.into(),
			block_hash: t.block_hash.into(),
		}
	}
}

/// Trace
#[derive(Debug)]
pub struct Trace {
	/// Trace address
	trace_address: Vec<usize>,
	/// Subtraces
	subtraces: usize,
	/// Action
	action: Action,
	/// Result
	result: Res,
}

impl Serialize for Trace {
	fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
		where S: Serializer
	{
		let mut state = serializer.serialize_struct("Trace", 4)?;
		match self.action {
			Action::Call(ref call) => {
				serializer.serialize_struct_elt(&mut state, "type", "call")?;
				serializer.serialize_struct_elt(&mut state, "action", call)?;
			},
			Action::Create(ref create) => {
				serializer.serialize_struct_elt(&mut state, "type", "create")?;
				serializer.serialize_struct_elt(&mut state, "action", create)?;
			},
			Action::Suicide(ref suicide) => {
				serializer.serialize_struct_elt(&mut state, "type", "suicide")?;
				serializer.serialize_struct_elt(&mut state, "action", suicide)?;
			},
		}

		match self.result {
			Res::Call(ref call) => serializer.serialize_struct_elt(&mut state, "result", call)?,
			Res::Create(ref create) => serializer.serialize_struct_elt(&mut state, "result", create)?,
			Res::FailedCall(ref error) => serializer.serialize_struct_elt(&mut state, "error", error.to_string())?,
			Res::FailedCreate(ref error) => serializer.serialize_struct_elt(&mut state, "error", error.to_string())?,
			Res::None => serializer.serialize_struct_elt(&mut state, "result", None as Option<u8>)?,
		}

		serializer.serialize_struct_elt(&mut state, "traceAddress", &self.trace_address)?;
		serializer.serialize_struct_elt(&mut state, "subtraces", &self.subtraces)?;

		serializer.serialize_struct_end(state)
	}
}

impl From<FlatTrace> for Trace {
	fn from(t: FlatTrace) -> Self {
		Trace {
			trace_address: t.trace_address.into_iter().map(Into::into).collect(),
			subtraces: t.subtraces.into(),
			action: t.action.into(),
			result: t.result.into(),
		}
	}
}

#[derive(Debug, Serialize)]
/// A diff of some chunk of memory.
pub struct TraceResults {
	/// The output of the call/create
	pub output: Bytes,
	/// The transaction trace.
	pub trace: Vec<Trace>,
	/// The transaction trace.
	#[serde(rename="vmTrace")]
	pub vm_trace: Option<VMTrace>,
	/// The transaction trace.
	#[serde(rename="stateDiff")]
	pub state_diff: Option<StateDiff>,
}

impl From<Executed> for TraceResults {
	fn from(t: Executed) -> Self {
		TraceResults {
			output: t.output.into(),
			trace: t.trace.into_iter().map(Into::into).collect(),
			vm_trace: t.vm_trace.map(Into::into),
			state_diff: t.state_diff.map(Into::into),
		}
	}
}

#[cfg(test)]
mod tests {
	use serde_json;
	use std::collections::BTreeMap;
	use v1::types::Bytes;
	use ethcore::trace::TraceError;
	use super::*;

	#[test]
	fn should_serialize_trace_results() {
		let r = TraceResults {
			output: vec![0x60].into(),
			trace: vec![],
			vm_trace: None,
			state_diff: None,
		};
		let serialized = serde_json::to_string(&r).unwrap();
		assert_eq!(serialized, r#"{"output":"0x60","trace":[],"vmTrace":null,"stateDiff":null}"#);
	}

	#[test]
	fn test_trace_call_serialize() {
		let t = LocalizedTrace {
			action: Action::Call(Call {
				from: 4.into(),
				to: 5.into(),
				value: 6.into(),
				gas: 7.into(),
				input: Bytes::new(vec![0x12, 0x34]),
				call_type: CallType::Call,
			}),
			result: Res::Call(CallResult {
				gas_used: 8.into(),
				output: vec![0x56, 0x78].into(),
			}),
			trace_address: vec![10],
			subtraces: 1,
			transaction_position: 11,
			transaction_hash: 12.into(),
			block_number: 13,
			block_hash: 14.into(),
		};
		let serialized = serde_json::to_string(&t).unwrap();
		assert_eq!(serialized, r#"{"type":"call","action":{"from":"0x0000000000000000000000000000000000000004","to":"0x0000000000000000000000000000000000000005","value":"0x6","gas":"0x7","input":"0x1234","callType":"call"},"result":{"gasUsed":"0x8","output":"0x5678"},"traceAddress":[10],"subtraces":1,"transactionPosition":11,"transactionHash":"0x000000000000000000000000000000000000000000000000000000000000000c","blockNumber":13,"blockHash":"0x000000000000000000000000000000000000000000000000000000000000000e"}"#);
	}

	#[test]
	fn test_trace_failed_call_serialize() {
		let t = LocalizedTrace {
			action: Action::Call(Call {
				from: 4.into(),
				to: 5.into(),
				value: 6.into(),
				gas: 7.into(),
				input: Bytes::new(vec![0x12, 0x34]),
				call_type: CallType::Call,
			}),
			result: Res::FailedCall(TraceError::OutOfGas),
			trace_address: vec![10],
			subtraces: 1,
			transaction_position: 11,
			transaction_hash: 12.into(),
			block_number: 13,
			block_hash: 14.into(),
		};
		let serialized = serde_json::to_string(&t).unwrap();
		assert_eq!(serialized, r#"{"type":"call","action":{"from":"0x0000000000000000000000000000000000000004","to":"0x0000000000000000000000000000000000000005","value":"0x6","gas":"0x7","input":"0x1234","callType":"call"},"error":"Out of gas","traceAddress":[10],"subtraces":1,"transactionPosition":11,"transactionHash":"0x000000000000000000000000000000000000000000000000000000000000000c","blockNumber":13,"blockHash":"0x000000000000000000000000000000000000000000000000000000000000000e"}"#);
	}

	#[test]
	fn test_trace_create_serialize() {
		let t = LocalizedTrace {
			action: Action::Create(Create {
				from: 4.into(),
				value: 6.into(),
				gas: 7.into(),
				init: Bytes::new(vec![0x12, 0x34]),
			}),
			result: Res::Create(CreateResult {
				gas_used: 8.into(),
				code: vec![0x56, 0x78].into(),
				address: 0xff.into(),
			}),
			trace_address: vec![10],
			subtraces: 1,
			transaction_position: 11,
			transaction_hash: 12.into(),
			block_number: 13,
			block_hash: 14.into(),
		};
		let serialized = serde_json::to_string(&t).unwrap();
		assert_eq!(serialized, r#"{"type":"create","action":{"from":"0x0000000000000000000000000000000000000004","value":"0x6","gas":"0x7","init":"0x1234"},"result":{"gasUsed":"0x8","code":"0x5678","address":"0x00000000000000000000000000000000000000ff"},"traceAddress":[10],"subtraces":1,"transactionPosition":11,"transactionHash":"0x000000000000000000000000000000000000000000000000000000000000000c","blockNumber":13,"blockHash":"0x000000000000000000000000000000000000000000000000000000000000000e"}"#);
	}

	#[test]
	fn test_trace_failed_create_serialize() {
		let t = LocalizedTrace {
			action: Action::Create(Create {
				from: 4.into(),
				value: 6.into(),
				gas: 7.into(),
				init: Bytes::new(vec![0x12, 0x34]),
			}),
			result: Res::FailedCreate(TraceError::OutOfGas),
			trace_address: vec![10],
			subtraces: 1,
			transaction_position: 11,
			transaction_hash: 12.into(),
			block_number: 13,
			block_hash: 14.into(),
		};
		let serialized = serde_json::to_string(&t).unwrap();
		assert_eq!(serialized, r#"{"type":"create","action":{"from":"0x0000000000000000000000000000000000000004","value":"0x6","gas":"0x7","init":"0x1234"},"error":"Out of gas","traceAddress":[10],"subtraces":1,"transactionPosition":11,"transactionHash":"0x000000000000000000000000000000000000000000000000000000000000000c","blockNumber":13,"blockHash":"0x000000000000000000000000000000000000000000000000000000000000000e"}"#);
	}

	#[test]
	fn test_trace_suicide_serialize() {
		let t = LocalizedTrace {
			action: Action::Suicide(Suicide {
				address: 4.into(),
				refund_address: 6.into(),
				balance: 7.into(),
			}),
			result: Res::None,
			trace_address: vec![10],
			subtraces: 1,
			transaction_position: 11,
			transaction_hash: 12.into(),
			block_number: 13,
			block_hash: 14.into(),
		};
		let serialized = serde_json::to_string(&t).unwrap();
		assert_eq!(serialized, r#"{"type":"suicide","action":{"address":"0x0000000000000000000000000000000000000004","refundAddress":"0x0000000000000000000000000000000000000006","balance":"0x7"},"result":null,"traceAddress":[10],"subtraces":1,"transactionPosition":11,"transactionHash":"0x000000000000000000000000000000000000000000000000000000000000000c","blockNumber":13,"blockHash":"0x000000000000000000000000000000000000000000000000000000000000000e"}"#);
	}

	#[test]
	fn test_vmtrace_serialize() {
		let t = VMTrace {
			code: vec![0, 1, 2, 3].into(),
			ops: vec![
				VMOperation {
					pc: 0,
					cost: 10,
					ex: None,
					sub: None,
				},
				VMOperation {
					pc: 1,
					cost: 11,
					ex: Some(VMExecutedOperation {
						used: 10,
						push: vec![69.into()],
						mem: None,
						store: None,
					}),
					sub: Some(VMTrace {
						code: vec![0].into(),
						ops: vec![
							VMOperation {
								pc: 0,
								cost: 0,
								ex: Some(VMExecutedOperation {
									used: 10,
									push: vec![42.into()].into(),
									mem: Some(MemoryDiff {off: 42, data: vec![1, 2, 3].into()}),
									store: Some(StorageDiff {key: 69.into(), val: 42.into()}),
								}),
								sub: None,
							}
						]
					}),
				}
			]
		};
		let serialized = serde_json::to_string(&t).unwrap();
		assert_eq!(serialized, r#"{"code":"0x00010203","ops":[{"pc":0,"cost":10,"ex":null,"sub":null},{"pc":1,"cost":11,"ex":{"used":10,"push":["0x45"],"mem":null,"store":null},"sub":{"code":"0x00","ops":[{"pc":0,"cost":0,"ex":{"used":10,"push":["0x2a"],"mem":{"off":42,"data":"0x010203"},"store":{"key":"0x45","val":"0x2a"}},"sub":null}]}}]}"#);
	}

	#[test]
	fn test_statediff_serialize() {
		let t = StateDiff(map![
			42.into() => AccountDiff {
				balance: Diff::Same,
				nonce: Diff::Born(1.into()),
				code: Diff::Same,
				storage: map![
					42.into() => Diff::Same
				]
			},
			69.into() => AccountDiff {
				balance: Diff::Same,
				nonce: Diff::Changed(ChangedType { from: 1.into(), to: 0.into() }),
				code: Diff::Died(vec![96].into()),
				storage: map![],
			}
		]);
		let serialized = serde_json::to_string(&t).unwrap();
		assert_eq!(serialized, r#"{"0x000000000000000000000000000000000000002a":{"balance":"=","nonce":{"+":"0x1"},"code":"=","storage":{"0x000000000000000000000000000000000000000000000000000000000000002a":"="}},"0x0000000000000000000000000000000000000045":{"balance":"=","nonce":{"*":{"from":"0x1","to":"0x0"}},"code":{"-":"0x60"},"storage":{}}}"#);
	}
}
