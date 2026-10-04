//! PocketIC receiver fixture: validate Candid decoding and observe the actual IC caller.

use candid::{CandidType, Principal};
use serde::Deserialize;
use std::cell::Cell;

thread_local! {
    static CALL_COUNT: Cell<u64> = const { Cell::new(0) };
}

#[derive(CandidType)]
struct EchoReply {
    caller: Principal,
    arg: Vec<u8>,
}

#[derive(CandidType, Deserialize)]
struct Envelope {
    version: u8,
    arg: Vec<u8>,
}

fn echo_reply() -> EchoReply {
    CALL_COUNT.with(|count| count.set(count.get() + 1));
    EchoReply {
        caller: ic_cdk::api::msg_caller(),
        arg: ic_cdk::api::msg_arg_data(),
    }
}

#[ic_cdk::update]
fn echo_raw(_number: u64, _text: String) -> EchoReply {
    echo_reply()
}

#[ic_cdk::update]
fn echo_envelope(envelope: Envelope) -> EchoReply {
    assert_eq!(envelope.version, 1);
    echo_reply()
}

#[ic_cdk::query]
fn call_count() -> u64 {
    CALL_COUNT.with(Cell::get)
}
