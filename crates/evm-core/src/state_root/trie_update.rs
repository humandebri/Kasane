//! どこで: state root差分計算 / 何を: journal生成 / なぜ: apply境界を明確化し通常経路の全件再構築を避けるため

use super::{
    build_anchor_delta, is_empty_trie_account, normalize_code_hash, AccountDelta, AnchorDelta,
    StorageRootUpdate, TrieDelta,
};
use crate::bytes::b256_to_bytes;
use crate::hash::keccak256;
use alloy_primitives::{Address, B256, U256};
use alloy_rlp::{Decodable, Encodable};
use alloy_trie::nodes::{BranchNode, ExtensionNode, LeafNode, RlpNode, TrieNode};
use alloy_trie::{Nibbles, TrieAccount, TrieMask, EMPTY_ROOT_HASH, KECCAK_EMPTY};
use evm_db::chain_data::HashKey;
use evm_db::stable_state::StableState;
use evm_db::types::keys::{
    make_account_key, make_storage_key, parse_account_key_bytes, parse_storage_key_bytes,
    AccountKey,
};
use smallvec::SmallVec;
use std::collections::{BTreeMap, BTreeSet};

pub type NodeDeltaCounts = BTreeMap<HashKey, i64>;
pub type NewNodeRecords = BTreeMap<HashKey, Vec<u8>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrieUpdateJournal {
    pub state_root: [u8; 32],
    pub storage_updates: Vec<StorageRootUpdate>,
    pub node_delta_counts: NodeDeltaCounts,
    pub new_node_records: NewNodeRecords,
    pub updated_account_leaf_hashes: BTreeMap<AccountKey, HashKey>,
    pub anchor_delta: AnchorDelta,
}

#[derive(Clone, Debug)]
struct KvOp {
    key: Nibbles,
    value: Option<SmallVec<[u8; 40]>>,
}

struct JournalBuilder<'a> {
    state: &'a StableState,
    node_delta_counts: NodeDeltaCounts,
    new_node_records: NewNodeRecords,
}

impl<'a> JournalBuilder<'a> {
    fn new(state: &'a StableState) -> Self {
        Self {
            state,
            node_delta_counts: BTreeMap::new(),
            new_node_records: BTreeMap::new(),
        }
    }

    fn resolve_ptr(&self, ptr: &RlpNode) -> Option<TrieNode> {
        let raw_owned: Vec<u8>;
        if let Some(hash) = ptr.as_hash() {
            let key = HashKey(b256_to_bytes(hash));
            raw_owned = if let Some(raw) = self.new_node_records.get(&key) {
                raw.clone()
            } else if let Some(record) = self.state.state_root_node_db.get(&key) {
                record.rlp.clone()
            } else {
                return None;
            };
            let mut slice = raw_owned.as_slice();
            TrieNode::decode(&mut slice).ok()
        } else {
            let mut slice = ptr.as_ref();
            TrieNode::decode(&mut slice).ok()
        }
    }

    fn emit_node(&mut self, node: TrieNode) -> RlpNode {
        let mut raw = Vec::with_capacity(96);
        let ptr = node.rlp(&mut raw);
        if let Some(hash) = ptr.as_hash() {
            self.new_node_records
                .entry(HashKey(b256_to_bytes(hash)))
                .or_insert(raw);
        }
        ptr
    }

    fn replace_ptr(&mut self, old_ptr: Option<&RlpNode>, new_ptr: Option<&RlpNode>) {
        let old_hash = old_ptr
            .and_then(|p| p.as_hash())
            .map(|h| HashKey(b256_to_bytes(h)));
        let new_hash = new_ptr
            .and_then(|p| p.as_hash())
            .map(|h| HashKey(b256_to_bytes(h)));
        if old_hash == new_hash {
            return;
        }
        if let Some(old) = old_hash {
            *self.node_delta_counts.entry(old).or_insert(0) -= 1;
            if self.node_delta_counts.get(&old) == Some(&0) {
                self.node_delta_counts.remove(&old);
            }
        }
        if let Some(new) = new_hash {
            *self.node_delta_counts.entry(new).or_insert(0) += 1;
            if self.node_delta_counts.get(&new) == Some(&0) {
                self.node_delta_counts.remove(&new);
            }
        }
    }
}

pub fn build_state_update_journal(
    state: &StableState,
    delta: &TrieDelta,
    touched_addrs: &[[u8; 20]],
) -> TrieUpdateJournal {
    let mut builder = JournalBuilder::new(state);

    let mut target_storage_addrs: BTreeSet<[u8; 20]> = BTreeSet::new();
    for addr in touched_addrs.iter().copied() {
        target_storage_addrs.insert(addr);
    }
    for (addr, account_delta) in delta.accounts.iter() {
        if account_delta.deleted || !account_delta.storage.is_empty() {
            target_storage_addrs.insert(*addr);
        }
    }

    let mut storage_root_after: BTreeMap<[u8; 20], B256> = BTreeMap::new();
    let mut storage_root_ptr_after: BTreeMap<[u8; 20], Option<RlpNode>> = BTreeMap::new();
    let mut storage_updates = Vec::new();

    for addr in target_storage_addrs {
        let account_key = make_account_key(addr);
        let old_root = state
            .state_storage_roots
            .get(&account_key)
            .map(|v| B256::from(v.0))
            .unwrap_or(EMPTY_ROOT_HASH);

        let mut new_root = old_root;
        let mut new_root_ptr = root_to_ptr(old_root);
        if let Some(account_delta) = delta.accounts.get(&addr) {
            if account_delta.deleted {
                new_root = EMPTY_ROOT_HASH;
                new_root_ptr = None;
            } else if !account_delta.storage.is_empty() {
                let mut ops = Vec::with_capacity(account_delta.storage.len());
                for (slot, value) in account_delta.storage.iter() {
                    let key_hash = keccak256(slot);
                    let key = Nibbles::unpack(key_hash);
                    let encoded = value.map(encode_u256_rlp);
                    ops.push(KvOp {
                        key,
                        value: encoded,
                    });
                }
                ops.sort_by(|a, b| a.key.cmp(&b.key));
                let next_ptr = apply_ops(&mut builder, new_root_ptr.as_ref(), &ops);
                new_root = ptr_to_root(next_ptr.as_ref());
                new_root_ptr = next_ptr;
            } else if touched_addrs.contains(&addr) {
                let full_ops = collect_storage_ops_from_state(state, addr);
                let rebuilt = apply_ops(&mut builder, None, &full_ops);
                new_root = ptr_to_root(rebuilt.as_ref());
                new_root_ptr = rebuilt;
            }
        } else if touched_addrs.contains(&addr) {
            let full_ops = collect_storage_ops_from_state(state, addr);
            let rebuilt = apply_ops(&mut builder, None, &full_ops);
            new_root = ptr_to_root(rebuilt.as_ref());
            new_root_ptr = rebuilt;
        }

        storage_root_after.insert(addr, new_root);
        storage_root_ptr_after.insert(addr, new_root_ptr);
        if new_root != old_root {
            storage_updates.push(StorageRootUpdate {
                addr,
                storage_root: if new_root == EMPTY_ROOT_HASH {
                    None
                } else {
                    Some(b256_to_bytes(new_root))
                },
            });
        }
    }

    let mut target_account_addrs: BTreeSet<[u8; 20]> = BTreeSet::new();
    for addr in delta.accounts.keys().copied() {
        target_account_addrs.insert(addr);
    }
    for addr in touched_addrs.iter().copied() {
        target_account_addrs.insert(addr);
    }
    for update in storage_updates.iter() {
        target_account_addrs.insert(update.addr);
    }

    let mut account_ops = Vec::with_capacity(target_account_addrs.len());
    for addr in target_account_addrs {
        let account_key = make_account_key(addr);
        let after = if let Some(account_delta) = delta.accounts.get(&addr) {
            account_after(state, account_delta, addr, &storage_root_after)
        } else {
            let storage_root = storage_root_after.get(&addr).copied().unwrap_or_else(|| {
                state
                    .state_storage_roots
                    .get(&account_key)
                    .map(|v| B256::from(v.0))
                    .unwrap_or(EMPTY_ROOT_HASH)
            });
            account_from_state(state, addr, storage_root)
        };
        let mut value = None;
        if let Some(account) = after {
            let mut out = Vec::with_capacity(128);
            account.encode(&mut out);
            value = Some(SmallVec::from_vec(out));
        }
        account_ops.push(KvOp {
            key: Nibbles::unpack(keccak256(Address::from(addr).as_slice())),
            value,
        });
    }
    account_ops.sort_by(|a, b| a.key.cmp(&b.key));

    let old_state_root = B256::from(state.state_root_meta.get().state_root);
    let state_root_ptr = root_to_ptr(old_state_root);
    let next_state_root_ptr = apply_ops(&mut builder, state_root_ptr.as_ref(), &account_ops);
    let new_state_root = ptr_to_root(next_state_root_ptr.as_ref());
    force_root_record(&mut builder, new_state_root, next_state_root_ptr.as_ref());
    for update in storage_updates.iter() {
        let root = update
            .storage_root
            .map(B256::from)
            .unwrap_or(EMPTY_ROOT_HASH);
        let ptr = storage_root_ptr_after
            .get(&update.addr)
            .and_then(|p| p.as_ref());
        force_root_record(&mut builder, root, ptr);
    }

    let anchor_delta = build_anchor_delta(state, &storage_updates, b256_to_bytes(new_state_root));

    TrieUpdateJournal {
        state_root: b256_to_bytes(new_state_root),
        storage_updates,
        node_delta_counts: builder.node_delta_counts,
        new_node_records: builder.new_node_records,
        updated_account_leaf_hashes: BTreeMap::new(),
        anchor_delta,
    }
}

fn force_root_record(builder: &mut JournalBuilder<'_>, root: B256, ptr: Option<&RlpNode>) {
    if root == EMPTY_ROOT_HASH {
        return;
    }
    let hash = HashKey(b256_to_bytes(root));
    if builder.new_node_records.contains_key(&hash) {
        return;
    }
    if let Some(record) = builder.state.state_root_node_db.get(&hash) {
        builder.new_node_records.insert(hash, record.rlp.clone());
        return;
    }
    if let Some(ptr) = ptr {
        if ptr.as_hash().is_none() {
            builder.new_node_records.insert(hash, ptr.as_ref().to_vec());
        }
    }
}

fn collect_storage_ops_from_state(state: &StableState, addr: [u8; 20]) -> Vec<KvOp> {
    let lower = make_storage_key(addr, [0u8; 32]);
    let upper = make_storage_key(addr, [0xffu8; 32]);
    let mut ops = Vec::new();
    for entry in state.storage.range(lower..=upper) {
        let key = entry.key().0;
        let Some((key_addr, slot)) = parse_storage_key_bytes(&key) else {
            break;
        };
        if key_addr != addr {
            break;
        }
        let out = encode_u256_rlp(entry.value().0);
        ops.push(KvOp {
            key: Nibbles::unpack(keccak256(&slot)),
            value: Some(out),
        });
    }
    ops.sort_by(|a, b| a.key.cmp(&b.key));
    ops
}

fn account_after(
    state: &StableState,
    delta: &AccountDelta,
    addr: [u8; 20],
    storage_roots: &BTreeMap<[u8; 20], B256>,
) -> Option<TrieAccount> {
    if delta.deleted {
        return None;
    }
    let storage_root = storage_roots.get(&addr).copied().unwrap_or_else(|| {
        state
            .state_storage_roots
            .get(&make_account_key(addr))
            .map(|v| B256::from(v.0))
            .unwrap_or(EMPTY_ROOT_HASH)
    });
    let account = TrieAccount {
        nonce: delta.nonce,
        balance: U256::from_be_bytes(delta.balance),
        storage_root,
        code_hash: normalize_code_hash(B256::from(delta.code_hash)),
    };
    if is_empty_trie_account(&account) {
        None
    } else {
        Some(account)
    }
}

fn account_from_state(
    state: &StableState,
    addr: [u8; 20],
    storage_root: B256,
) -> Option<TrieAccount> {
    let key = make_account_key(addr);
    let account = if let Some(a) = state.accounts.get(&key) {
        TrieAccount {
            nonce: a.nonce(),
            balance: U256::from_be_bytes(a.balance()),
            storage_root,
            code_hash: normalize_code_hash(B256::from(a.code_hash())),
        }
    } else {
        TrieAccount {
            nonce: 0,
            balance: U256::ZERO,
            storage_root,
            code_hash: KECCAK_EMPTY,
        }
    };
    if is_empty_trie_account(&account) {
        None
    } else {
        Some(account)
    }
}

fn root_to_ptr(root: B256) -> Option<RlpNode> {
    if root == EMPTY_ROOT_HASH {
        None
    } else {
        Some(RlpNode::word_rlp(&root))
    }
}

fn ptr_to_root(ptr: Option<&RlpNode>) -> B256 {
    match ptr {
        Some(ptr) => {
            if let Some(hash) = ptr.as_hash() {
                hash
            } else {
                B256::from(keccak256(ptr.as_ref()))
            }
        }
        None => EMPTY_ROOT_HASH,
    }
}

fn apply_ops(
    builder: &mut JournalBuilder<'_>,
    root: Option<&RlpNode>,
    ops: &[KvOp],
) -> Option<RlpNode> {
    let mut current = root.cloned();
    for op in ops {
        current = apply_op(builder, current.as_ref(), &op.key, op.value.as_deref());
    }
    current
}

fn apply_op(
    builder: &mut JournalBuilder<'_>,
    root: Option<&RlpNode>,
    key: &Nibbles,
    value: Option<&[u8]>,
) -> Option<RlpNode> {
    let next = update_at(builder, root, key, 0, value);
    builder.replace_ptr(root, next.as_ref());
    next
}

fn update_at(
    builder: &mut JournalBuilder<'_>,
    node_ptr: Option<&RlpNode>,
    key: &Nibbles,
    depth: usize,
    value: Option<&[u8]>,
) -> Option<RlpNode> {
    let rest = key.slice(depth..);
    let Some(ptr) = node_ptr else {
        return value.map(|v| builder.emit_node(TrieNode::Leaf(LeafNode::new(rest, v.to_vec()))));
    };
    let Some(node) = builder.resolve_ptr(ptr) else {
        return node_ptr.cloned();
    };

    match node {
        TrieNode::EmptyRoot => {
            value.map(|v| builder.emit_node(TrieNode::Leaf(LeafNode::new(rest, v.to_vec()))))
        }
        TrieNode::Leaf(leaf) => update_leaf(builder, ptr, leaf, &rest, value),
        TrieNode::Extension(ext) => update_extension(builder, ptr, ext, key, depth, value),
        TrieNode::Branch(branch) => update_branch(builder, ptr, branch, key, depth, value),
    }
}

fn update_leaf(
    builder: &mut JournalBuilder<'_>,
    old_ptr: &RlpNode,
    leaf: LeafNode,
    rest: &Nibbles,
    value: Option<&[u8]>,
) -> Option<RlpNode> {
    let common = leaf.key.common_prefix_length(rest);
    if common == leaf.key.len() && common == rest.len() {
        let Some(v) = value else {
            builder.replace_ptr(Some(old_ptr), None);
            return None;
        };
        if leaf.value.as_slice() == v {
            return Some(old_ptr.clone());
        }
        let next = builder.emit_node(TrieNode::Leaf(LeafNode::new(leaf.key, v.to_vec())));
        builder.replace_ptr(Some(old_ptr), Some(&next));
        return Some(next);
    }

    let mut children: [Option<RlpNode>; 16] = std::array::from_fn(|_| None);
    let old_suffix = leaf.key.slice(common..);
    if !old_suffix.is_empty() {
        let old_idx = old_suffix.get(0).unwrap() as usize;
        let old_tail = old_suffix.slice(1..);
        let old_child = builder.emit_node(TrieNode::Leaf(LeafNode::new(old_tail, leaf.value)));
        children[old_idx] = Some(old_child);
    }

    if let Some(v) = value {
        let new_suffix = rest.slice(common..);
        if !new_suffix.is_empty() {
            let new_idx = new_suffix.get(0).unwrap() as usize;
            let new_tail = new_suffix.slice(1..);
            let new_child = builder.emit_node(TrieNode::Leaf(LeafNode::new(new_tail, v.to_vec())));
            children[new_idx] = Some(new_child);
        }
    }

    let Some(collapsed) = collapse_children(builder, children) else {
        builder.replace_ptr(Some(old_ptr), None);
        return None;
    };

    let next = if common > 0 {
        let prefix = rest.slice(0..common);
        builder.emit_node(TrieNode::Extension(ExtensionNode::new(prefix, collapsed)))
    } else {
        collapsed
    };
    builder.replace_ptr(Some(old_ptr), Some(&next));
    Some(next)
}

fn update_extension(
    builder: &mut JournalBuilder<'_>,
    old_ptr: &RlpNode,
    ext: ExtensionNode,
    key: &Nibbles,
    depth: usize,
    value: Option<&[u8]>,
) -> Option<RlpNode> {
    let rest = key.slice(depth..);
    let common = ext.key.common_prefix_length(&rest);

    if common == ext.key.len() {
        let child_next = update_at(builder, Some(&ext.child), key, depth + common, value);
        let Some(child_next) = child_next else {
            builder.replace_ptr(Some(old_ptr), None);
            return None;
        };
        let next = if ext.key.is_empty() {
            child_next
        } else {
            builder.emit_node(TrieNode::Extension(ExtensionNode::new(ext.key, child_next)))
        };
        builder.replace_ptr(Some(old_ptr), Some(&next));
        return Some(next);
    }

    let mut children: [Option<RlpNode>; 16] = std::array::from_fn(|_| None);

    let old_suffix = ext.key.slice(common..);
    let old_idx = old_suffix.get(0).unwrap() as usize;
    let old_tail = old_suffix.slice(1..);
    let old_child = if old_tail.is_empty() {
        ext.child
    } else {
        builder.emit_node(TrieNode::Extension(ExtensionNode::new(old_tail, ext.child)))
    };
    children[old_idx] = Some(old_child);

    if let Some(v) = value {
        let new_suffix = rest.slice(common..);
        if !new_suffix.is_empty() {
            let new_idx = new_suffix.get(0).unwrap() as usize;
            let new_tail = new_suffix.slice(1..);
            let new_child = builder.emit_node(TrieNode::Leaf(LeafNode::new(new_tail, v.to_vec())));
            children[new_idx] = Some(new_child);
        }
    }

    let Some(collapsed) = collapse_children(builder, children) else {
        builder.replace_ptr(Some(old_ptr), None);
        return None;
    };
    let next = if common > 0 {
        builder.emit_node(TrieNode::Extension(ExtensionNode::new(
            rest.slice(0..common),
            collapsed,
        )))
    } else {
        collapsed
    };
    builder.replace_ptr(Some(old_ptr), Some(&next));
    Some(next)
}

fn update_branch(
    builder: &mut JournalBuilder<'_>,
    old_ptr: &RlpNode,
    branch: BranchNode,
    key: &Nibbles,
    depth: usize,
    value: Option<&[u8]>,
) -> Option<RlpNode> {
    if depth >= key.len() {
        return Some(old_ptr.clone());
    }

    let mut children = branch_children(&branch);
    let index = key.get(depth).unwrap() as usize;
    let next_child = update_at(builder, children[index].as_ref(), key, depth + 1, value);
    children[index] = next_child;

    let Some(collapsed) = collapse_children(builder, children) else {
        builder.replace_ptr(Some(old_ptr), None);
        return None;
    };
    builder.replace_ptr(Some(old_ptr), Some(&collapsed));
    Some(collapsed)
}

fn branch_children(branch: &BranchNode) -> [Option<RlpNode>; 16] {
    let mut out: [Option<RlpNode>; 16] = std::array::from_fn(|_| None);
    let mut pos = 0usize;
    for idx in 0..16u8 {
        if branch.state_mask.is_bit_set(idx) {
            out[idx as usize] = branch.stack.get(pos).cloned();
            pos = pos.saturating_add(1);
        }
    }
    out
}

fn collapse_children(
    builder: &mut JournalBuilder<'_>,
    children: [Option<RlpNode>; 16],
) -> Option<RlpNode> {
    let mut present = Vec::new();
    for (idx, child) in children.iter().enumerate() {
        if child.is_some() {
            present.push((idx as u8, child.clone().unwrap()));
        }
    }

    match present.len() {
        0 => None,
        1 => {
            let (idx, child) = &present[0];
            let Some(child_node) = builder.resolve_ptr(child) else {
                let prefix = Nibbles::from_nibbles_unchecked([*idx]);
                return Some(builder.emit_node(TrieNode::Extension(ExtensionNode::new(
                    prefix,
                    child.clone(),
                ))));
            };
            let prefix = Nibbles::from_nibbles_unchecked([*idx]);
            match child_node {
                TrieNode::Leaf(leaf) => {
                    let key = prefix.join(&leaf.key);
                    Some(builder.emit_node(TrieNode::Leaf(LeafNode::new(key, leaf.value))))
                }
                TrieNode::Extension(ext) => {
                    let key = prefix.join(&ext.key);
                    Some(builder.emit_node(TrieNode::Extension(ExtensionNode::new(key, ext.child))))
                }
                _ => Some(builder.emit_node(TrieNode::Extension(ExtensionNode::new(
                    prefix,
                    child.clone(),
                )))),
            }
        }
        _ => {
            let mut stack = Vec::with_capacity(present.len());
            let mut mask = TrieMask::default();
            for (idx, child) in present {
                mask.set_bit(idx);
                stack.push(child);
            }
            Some(builder.emit_node(TrieNode::Branch(BranchNode::new(stack, mask))))
        }
    }
}

fn encode_u256_rlp(value: [u8; 32]) -> SmallVec<[u8; 40]> {
    let mut out = Vec::with_capacity(33);
    U256::from_be_bytes(value).encode(&mut out);
    SmallVec::from_vec(out)
}

fn rlp_node_hash(ptr: &RlpNode) -> HashKey {
    if let Some(hash) = ptr.as_hash() {
        HashKey(b256_to_bytes(hash))
    } else {
        HashKey(keccak256(ptr.as_ref()))
    }
}

fn branch_child(branch: &BranchNode, nibble: u8) -> Option<RlpNode> {
    if !branch.state_mask.is_bit_set(nibble) {
        return None;
    }
    let mut pos = 0usize;
    for idx in 0..16u8 {
        if idx == nibble {
            return branch.stack.get(pos).cloned();
        }
        if branch.state_mask.is_bit_set(idx) {
            pos = pos.saturating_add(1);
        }
    }
    None
}

fn resolve_leaf_hash_for_key(
    builder: &JournalBuilder<'_>,
    root_ptr: &RlpNode,
    key: &Nibbles,
) -> Option<HashKey> {
    let mut depth = 0usize;
    let mut current = root_ptr.clone();
    loop {
        let node = builder.resolve_ptr(&current)?;
        match node {
            TrieNode::Leaf(leaf) => {
                if leaf.key == key.slice(depth..) {
                    return Some(rlp_node_hash(&current));
                }
                return None;
            }
            TrieNode::Extension(ext) => {
                let rest = key.slice(depth..);
                let common = ext.key.common_prefix_length(&rest);
                if common != ext.key.len() {
                    return None;
                }
                depth = depth.saturating_add(common);
                current = ext.child;
            }
            TrieNode::Branch(branch) => {
                if depth >= key.len() {
                    return None;
                }
                let nibble = key.get(depth)?;
                let child = branch_child(&branch, nibble)?;
                depth = depth.saturating_add(1);
                current = child;
            }
            TrieNode::EmptyRoot => return None,
        }
    }
}

pub fn build_state_update_journal_full(
    state: &StableState,
    _delta: &TrieDelta,
    _storage_updates: Vec<StorageRootUpdate>,
) -> TrieUpdateJournal {
    let mut builder = JournalBuilder::new(state);

    let mut storage_addrs = BTreeSet::new();
    for entry in state.storage.iter() {
        if let Some((addr, _)) = parse_storage_key_bytes(&entry.key().0) {
            storage_addrs.insert(addr);
        }
    }

    let mut storage_root_after: BTreeMap<[u8; 20], B256> = BTreeMap::new();
    let mut storage_root_ptr_after: BTreeMap<[u8; 20], Option<RlpNode>> = BTreeMap::new();
    for addr in storage_addrs {
        let ops = collect_storage_ops_from_state(state, addr);
        let next_ptr = apply_ops(&mut builder, None, &ops);
        let root = ptr_to_root(next_ptr.as_ref());
        storage_root_after.insert(addr, root);
        storage_root_ptr_after.insert(addr, next_ptr);
    }

    let mut target_account_addrs: BTreeSet<[u8; 20]> = BTreeSet::new();
    for entry in state.accounts.iter() {
        if let Some(addr) = parse_account_key_bytes(&entry.key().0) {
            target_account_addrs.insert(addr);
        }
    }
    for entry in state.state_storage_roots.iter() {
        if let Some(addr) = parse_account_key_bytes(&entry.key().0) {
            target_account_addrs.insert(addr);
        }
    }
    for addr in storage_root_after.keys().copied() {
        target_account_addrs.insert(addr);
    }

    let mut account_ops = Vec::with_capacity(target_account_addrs.len());
    let mut account_key_nibbles = Vec::with_capacity(target_account_addrs.len());
    for addr in target_account_addrs {
        let account_key = make_account_key(addr);
        let storage_root = storage_root_after.get(&addr).copied().unwrap_or_else(|| {
            state
                .state_storage_roots
                .get(&account_key)
                .map(|v| B256::from(v.0))
                .unwrap_or(EMPTY_ROOT_HASH)
        });
        let mut value = None;
        if let Some(account) = account_from_state(state, addr, storage_root) {
            let mut out = Vec::with_capacity(128);
            account.encode(&mut out);
            value = Some(SmallVec::from_vec(out));
        }
        let key = Nibbles::unpack(keccak256(Address::from(addr).as_slice()));
        account_key_nibbles.push((addr, key.clone()));
        account_ops.push(KvOp { key, value });
    }
    account_ops.sort_by(|a, b| a.key.cmp(&b.key));

    let state_root_ptr = apply_ops(&mut builder, None, &account_ops);
    let state_root = ptr_to_root(state_root_ptr.as_ref());
    force_root_record(&mut builder, state_root, state_root_ptr.as_ref());
    for (addr, root) in storage_root_after.iter() {
        let ptr = storage_root_ptr_after.get(addr).and_then(|v| v.as_ref());
        force_root_record(&mut builder, *root, ptr);
    }

    let mut updated_account_leaf_hashes = BTreeMap::new();
    if let Some(root_ptr) = state_root_ptr.as_ref() {
        for (addr, key) in account_key_nibbles {
            if let Some(hash) = resolve_leaf_hash_for_key(&builder, root_ptr, &key) {
                updated_account_leaf_hashes.insert(make_account_key(addr), hash);
            }
        }
    }

    TrieUpdateJournal {
        state_root: b256_to_bytes(state_root),
        storage_updates: Vec::new(),
        node_delta_counts: builder.node_delta_counts,
        new_node_records: builder.new_node_records,
        updated_account_leaf_hashes,
        anchor_delta: AnchorDelta::default(),
    }
}
