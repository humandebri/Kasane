// Diagnostic corpus generated from the fixed actual revm instruction table.
// DummyHost is a probe type; host implementation/refinement is not proved.
use revm_interpreter::{
    host::DummyHost, interpreter::EthInterpreter, InstructionContext, Interpreter,
};

pub fn opcode_stop(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::control::stop(InstructionContext { interpreter, host });
}

pub fn opcode_add(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::arithmetic::add(InstructionContext { interpreter, host });
}

pub fn opcode_mul(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::arithmetic::mul(InstructionContext { interpreter, host });
}

pub fn opcode_sub(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::arithmetic::sub(InstructionContext { interpreter, host });
}

pub fn opcode_div(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::arithmetic::div(InstructionContext { interpreter, host });
}

pub fn opcode_sdiv(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::arithmetic::sdiv(InstructionContext { interpreter, host });
}

pub fn opcode_mod(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::arithmetic::rem(InstructionContext { interpreter, host });
}

pub fn opcode_smod(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::arithmetic::smod(InstructionContext { interpreter, host });
}

pub fn opcode_addmod(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::arithmetic::addmod(InstructionContext { interpreter, host });
}

pub fn opcode_mulmod(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::arithmetic::mulmod(InstructionContext { interpreter, host });
}

pub fn opcode_exp(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::arithmetic::exp(InstructionContext { interpreter, host });
}

pub fn opcode_signextend(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::arithmetic::signextend(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_lt(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::lt(InstructionContext { interpreter, host });
}

pub fn opcode_gt(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::gt(InstructionContext { interpreter, host });
}

pub fn opcode_slt(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::slt(InstructionContext { interpreter, host });
}

pub fn opcode_sgt(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::sgt(InstructionContext { interpreter, host });
}

pub fn opcode_eq(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::eq(InstructionContext { interpreter, host });
}

pub fn opcode_iszero(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::iszero(InstructionContext { interpreter, host });
}

pub fn opcode_and(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::bitand(InstructionContext { interpreter, host });
}

pub fn opcode_or(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::bitor(InstructionContext { interpreter, host });
}

pub fn opcode_xor(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::bitxor(InstructionContext { interpreter, host });
}

pub fn opcode_not(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::not(InstructionContext { interpreter, host });
}

pub fn opcode_byte(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::byte(InstructionContext { interpreter, host });
}

pub fn opcode_shl(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::shl(InstructionContext { interpreter, host });
}

pub fn opcode_shr(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::shr(InstructionContext { interpreter, host });
}

pub fn opcode_sar(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::sar(InstructionContext { interpreter, host });
}

pub fn opcode_clz(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::bitwise::clz(InstructionContext { interpreter, host });
}

pub fn opcode_keccak256(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::system::keccak256(InstructionContext { interpreter, host });
}

pub fn opcode_address(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::system::address(InstructionContext { interpreter, host });
}

pub fn opcode_balance(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::balance(InstructionContext { interpreter, host });
}

pub fn opcode_origin(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::tx_info::origin(InstructionContext { interpreter, host });
}

pub fn opcode_caller(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::system::caller(InstructionContext { interpreter, host });
}

pub fn opcode_callvalue(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::system::callvalue(InstructionContext { interpreter, host });
}

pub fn opcode_calldataload(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::system::calldataload(InstructionContext { interpreter, host });
}

pub fn opcode_calldatasize(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::system::calldatasize(InstructionContext { interpreter, host });
}

pub fn opcode_calldatacopy(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::system::calldatacopy(InstructionContext { interpreter, host });
}

pub fn opcode_codesize(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::system::codesize(InstructionContext { interpreter, host });
}

pub fn opcode_codecopy(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::system::codecopy(InstructionContext { interpreter, host });
}

pub fn opcode_gasprice(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::tx_info::gasprice(InstructionContext { interpreter, host });
}

pub fn opcode_extcodesize(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::extcodesize(InstructionContext { interpreter, host });
}

pub fn opcode_extcodecopy(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::extcodecopy(InstructionContext { interpreter, host });
}

pub fn opcode_returndatasize(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::system::returndatasize(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_returndatacopy(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::system::returndatacopy(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_extcodehash(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::extcodehash(InstructionContext { interpreter, host });
}

pub fn opcode_blockhash(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::blockhash(InstructionContext { interpreter, host });
}

pub fn opcode_coinbase(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::block_info::coinbase(InstructionContext { interpreter, host });
}

pub fn opcode_timestamp(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::block_info::timestamp(InstructionContext { interpreter, host });
}

pub fn opcode_number(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::block_info::block_number(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_difficulty(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::block_info::difficulty(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_gaslimit(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::block_info::gaslimit(InstructionContext { interpreter, host });
}

pub fn opcode_chainid(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::block_info::chainid(InstructionContext { interpreter, host });
}

pub fn opcode_selfbalance(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::selfbalance(InstructionContext { interpreter, host });
}

pub fn opcode_basefee(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::block_info::basefee(InstructionContext { interpreter, host });
}

pub fn opcode_blobhash(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::tx_info::blob_hash(InstructionContext { interpreter, host });
}

pub fn opcode_blobbasefee(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::block_info::blob_basefee(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_pop(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::pop(InstructionContext { interpreter, host });
}

pub fn opcode_mload(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::memory::mload(InstructionContext { interpreter, host });
}

pub fn opcode_mstore(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::memory::mstore(InstructionContext { interpreter, host });
}

pub fn opcode_mstore8(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::memory::mstore8(InstructionContext { interpreter, host });
}

pub fn opcode_sload(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::sload(InstructionContext { interpreter, host });
}

pub fn opcode_sstore(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::sstore(InstructionContext { interpreter, host });
}

pub fn opcode_jump(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::control::jump(InstructionContext { interpreter, host });
}

pub fn opcode_jumpi(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::control::jumpi(InstructionContext { interpreter, host });
}

pub fn opcode_pc(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::control::pc(InstructionContext { interpreter, host });
}

pub fn opcode_msize(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::memory::msize(InstructionContext { interpreter, host });
}

pub fn opcode_gas(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::system::gas(InstructionContext { interpreter, host });
}

pub fn opcode_jumpdest(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::control::jumpdest(InstructionContext { interpreter, host });
}

pub fn opcode_tload(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::tload(InstructionContext { interpreter, host });
}

pub fn opcode_tstore(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::tstore(InstructionContext { interpreter, host });
}

pub fn opcode_mcopy(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::memory::mcopy(InstructionContext { interpreter, host });
}

pub fn opcode_push0(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push0(InstructionContext { interpreter, host });
}

pub fn opcode_push1(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<1, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push2(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<2, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push3(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<3, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push4(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<4, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push5(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<5, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push6(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<6, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push7(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<7, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push8(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<8, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push9(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<9, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push10(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<10, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push11(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<11, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push12(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<12, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push13(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<13, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push14(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<14, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push15(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<15, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push16(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<16, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push17(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<17, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push18(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<18, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push19(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<19, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push20(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<20, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push21(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<21, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push22(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<22, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push23(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<23, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push24(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<24, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push25(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<25, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push26(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<26, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push27(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<27, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push28(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<28, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push29(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<29, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push30(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<30, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push31(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<31, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_push32(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::push::<32, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_dup1(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<1, _, _>(InstructionContext { interpreter, host });
}

pub fn opcode_dup2(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<2, _, _>(InstructionContext { interpreter, host });
}

pub fn opcode_dup3(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<3, _, _>(InstructionContext { interpreter, host });
}

pub fn opcode_dup4(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<4, _, _>(InstructionContext { interpreter, host });
}

pub fn opcode_dup5(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<5, _, _>(InstructionContext { interpreter, host });
}

pub fn opcode_dup6(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<6, _, _>(InstructionContext { interpreter, host });
}

pub fn opcode_dup7(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<7, _, _>(InstructionContext { interpreter, host });
}

pub fn opcode_dup8(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<8, _, _>(InstructionContext { interpreter, host });
}

pub fn opcode_dup9(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<9, _, _>(InstructionContext { interpreter, host });
}

pub fn opcode_dup10(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<10, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_dup11(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<11, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_dup12(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<12, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_dup13(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<13, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_dup14(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<14, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_dup15(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<15, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_dup16(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::dup::<16, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap1(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<1, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap2(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<2, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap3(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<3, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap4(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<4, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap5(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<5, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap6(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<6, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap7(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<7, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap8(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<8, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap9(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<9, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap10(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<10, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap11(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<11, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap12(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<12, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap13(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<13, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap14(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<14, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap15(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<15, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_swap16(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::stack::swap::<16, _, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_log0(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::log::<0, _>(InstructionContext { interpreter, host });
}

pub fn opcode_log1(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::log::<1, _>(InstructionContext { interpreter, host });
}

pub fn opcode_log2(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::log::<2, _>(InstructionContext { interpreter, host });
}

pub fn opcode_log3(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::log::<3, _>(InstructionContext { interpreter, host });
}

pub fn opcode_log4(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::log::<4, _>(InstructionContext { interpreter, host });
}

pub fn opcode_create(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::contract::create::<_, false, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_call(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::contract::call(InstructionContext { interpreter, host });
}

pub fn opcode_callcode(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::contract::call_code(InstructionContext { interpreter, host });
}

pub fn opcode_return(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::control::ret(InstructionContext { interpreter, host });
}

pub fn opcode_delegatecall(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::contract::delegate_call(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_create2(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::contract::create::<_, true, _>(InstructionContext {
        interpreter,
        host,
    });
}

pub fn opcode_staticcall(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::contract::static_call(InstructionContext { interpreter, host });
}

pub fn opcode_revert(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::control::revert(InstructionContext { interpreter, host });
}

pub fn opcode_invalid(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::control::invalid(InstructionContext { interpreter, host });
}

pub fn opcode_selfdestruct(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::host::selfdestruct(InstructionContext { interpreter, host });
}

pub fn opcode_unknown(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {
    revm_interpreter::instructions::control::unknown(InstructionContext { interpreter, host });
}
