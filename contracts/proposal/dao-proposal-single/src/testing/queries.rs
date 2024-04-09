use cosmwasm_std::Addr;
use secret_multi_test::App;

use crate::msg::QueryMsg;

pub(crate) fn query_next_proposal_id(
    app: &App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
) -> u64 {
    app.wrap()
        .query_wasm_smart(
            proposal_single_code_hash,
            proposal_single,
            &QueryMsg::NextProposalId {},
        )
        .unwrap()
}
