use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use secret_toolkit::utils::HandleCallback;

#[cw_serde]
pub enum VotingCW4ExecuteMsg {
    AddGroupContract { addr: Addr, code_hash: String },
}

impl HandleCallback for VotingCW4ExecuteMsg {
    const BLOCK_SIZE: usize = 256;
}
