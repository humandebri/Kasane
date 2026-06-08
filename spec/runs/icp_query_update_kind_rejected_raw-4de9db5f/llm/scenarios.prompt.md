Generate scenario candidates:
#[cfg_attr(verus_keep_ghost, verus_spec(rejected => ensures
    rejected == (kind == ICP_QUERY_KIND_UPDATE_RESERVED),
))]
pub fn icp_query_update_kind_rejected_raw(kind: u64) -> bool
{
    kind == ICP_QUERY_KIND_UPDATE_RESERVED
}
