use cosmwasm_std::Addr;
use secret_multi_test::App;

use crate::msg::QueryMsg;

pub(crate) fn query_next_proposal_id(
    app: &App,
    proposal_multiple: &Addr,
    proposal_multiple_code_hash: String,
) -> u64 {
    app.wrap()
        .query_wasm_smart(
            proposal_multiple_code_hash,
            proposal_multiple,
            &QueryMsg::NextProposalId {},
        )
        .unwrap()
}
