//! どこで: Candid固定化 / 何を: did出力 / なぜ: wire互換の差分を可視化するため

fn main() {
    let did = ic_evm_gateway::export_did();
    println!("{}", did);
}
