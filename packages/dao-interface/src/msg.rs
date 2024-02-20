use cosmwasm_schema:: QueryResponses;
use cosmwasm_std::{Addr, Binary, CosmosMsg, Empty, Uint128};
use schemars::JsonSchema;
use secret_toolkit::utils::HandleCallback;
use secret_utils::Duration;
use serde::{Deserialize, Serialize};
use crate::state::Config;
use crate::{migrate_msg::MigrateParams, query::SubDao, state::ModuleInstantiateInfo};

/// Information about an item to be stored in the items list.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InitialItem {
    /// The name of the item.
    pub key: String,
    /// The value the item will have at instantiation time.
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Optional Admin with the ability to execute DAO messages
    /// directly. Useful for building SubDAOs controlled by a parent
    /// DAO. If no admin is specified the contract is set as its own
    /// admin so that the admin may be updated later by governance.
    pub admin: Option<String>,
    /// The name of the core contract.
    pub name: String,
    /// A description of the core contract.
    pub description: String,
    /// An image URL to describe the core module contract.
    pub image_url: Option<String>,

    /// If true the contract will automatically add received snip20
    /// tokens to its treasury.
    pub automatically_add_snip20s: bool,
    /// If true the contract will automatically add received snip721
    /// tokens to its treasury.
    pub automatically_add_snip721s: bool,

    /// Instantiate information for the core contract's voting
    /// power module.
    pub voting_module_instantiate_info: ModuleInstantiateInfo,
    /// Instantiate information for the core contract's proposal modules.
    /// NOTE: the pre-propose-base package depends on it being the case
    /// that the core module instantiates its proposal module.
    pub proposal_modules_instantiate_info: Vec<ModuleInstantiateInfo>,

    /// The items to instantiate this DAO with. Items are arbitrary
    /// key-value pairs whose contents are controlled by governance.
    ///
    /// It is an error to provide two items with the same key.
    pub initial_items: Option<Vec<InitialItem>>,
    /// Implements the DAO Star standard: <https://daostar.one/EIP>
    pub dao_uri: Option<String>,
    pub snip20_code_hash: String,
    pub snip721_code_hash: String,
}


/// Snip20ReceiveMsg should be de/serialized under `Receive()` variant in a HandleMsg
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Snip20ReceiveMsg {
    pub sender: Addr,
    pub from: Addr,
    pub amount: Uint128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    pub msg: Option<Binary>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Snip721ReceiveMsg {
    /// ReceiveNft may be a HandleMsg variant of any contract that wants to implement a receiver
    /// interface.  BatchReceiveNft, which is more informative and more efficient, is preferred over
    /// ReceiveNft.  Please read above regarding why ReceiveNft, which follows CW-721 standard has an
    /// inaccurately named `sender` field
    ReceiveNft {
        /// previous owner of sent token
        sender: Addr,
        /// token that was sent
        token_id: String,
        /// optional message to control receiving logic
        msg: Option<Binary>,
    },
    /// BatchReceiveNft may be a HandleMsg variant of any contract that wants to implement a receiver
    /// interface.  BatchReceiveNft, which is more informative and more efficient, is preferred over
    /// ReceiveNft.
    BatchReceiveNft {
        /// address that sent the tokens.  There is no ReceiveNft field equivalent to this
        sender: Addr,
        /// previous owner of sent tokens.  This is equivalent to the ReceiveNft `sender` field
        from: Addr,
        /// tokens that were sent
        token_ids: Vec<String>,
        /// optional message to control receiving logic
        msg: Option<Binary>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Callable by the Admin, if one is configured.
    /// Executes messages in order.
    ExecuteAdminMsgs { msgs: Vec<CosmosMsg<Empty>> },
    /// Callable by proposal modules. The DAO will execute the
    /// messages in the hook in order.
    ExecuteProposalHook { msgs: Vec<CosmosMsg<Empty>> },
    /// Pauses the DAO for a set duration.
    /// When paused the DAO is unable to execute proposals
    Pause { duration: Duration },
    /// Executed when the contract receives a cw20 token. Depending on
    /// the contract's configuration the contract will automatically
    /// add the token to its treasury.
    Receive(Snip20ReceiveMsg),
    /// Executed when the contract receives a cw721 token. Depending
    /// on the contract's configuration the contract will
    /// automatically add the token to its treasury.
    ReceiveNft(Snip721ReceiveMsg),
    /// Removes an item from the governance contract's item map.
    RemoveItem { key: String },
    /// Adds an item to the governance contract's item map. If the
    /// item already exists the existing value is overridden. If the
    /// item does not exist a new item is added.
    SetItem { key: String, value: String },
    /// Callable by the admin of the contract. If ADMIN is None the
    /// admin is set as the contract itself so that it may be updated
    /// later by vote. If ADMIN is Some a new admin is proposed and
    /// that new admin may become the admin by executing the
    /// `AcceptAdminNomination` message.
    ///
    /// If there is already a pending admin nomination the
    /// `WithdrawAdminNomination` message must be executed before a
    /// new admin may be nominated.
    NominateAdmin { admin: Option<String> },
    /// Callable by a nominated admin. Admins are nominated via the
    /// `NominateAdmin` message. Accepting a nomination will make the
    /// nominated address the new admin.
    ///
    /// Requiring that the new admin accepts the nomination before
    /// becoming the admin protects against a typo causing the admin
    /// to change to an invalid address.
    AcceptAdminNomination {},
    /// Callable by the current admin. Withdraws the current admin
    /// nomination.
    WithdrawAdminNomination {},
    /// Callable by the core contract. Replaces the current
    /// governance contract config with the provided config.
    UpdateConfig { config: Config },
    /// Updates the list of cw20 tokens this contract has registered.
    UpdateSnip20List {
        to_add: Vec<String>,
        to_remove: Vec<String>,
    },
    /// Updates the list of cw721 tokens this contract has registered.
    UpdateSnip721List {
        to_add: Vec<String>,
        to_remove: Vec<String>,
    },
    /// Updates the governance contract's governance modules. Module
    /// instantiate info in `to_add` is used to create new modules and
    /// install them.
    UpdateProposalModules {
        /// NOTE: the pre-propose-base package depends on it being the
        /// case that the core module instantiates its proposal module.
        to_add: Vec<ModuleInstantiateInfo>,
        to_disable: Vec<String>,
    },
    /// Callable by the core contract. Replaces the current
    /// voting module with a new one instantiated by the governance
    /// contract.
    UpdateVotingModule { module: ModuleInstantiateInfo },
    /// Update the core module to add/remove SubDAOs and their charters
    UpdateSubDaos {
        to_add: Vec<SubDao>,
        to_remove: Vec<String>,
    },
}

impl HandleCallback for ExecuteMsg {
    const BLOCK_SIZE: usize=256;
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Get's the DAO's admin. Returns `Addr`.
    #[returns(cosmwasm_std::Addr)]
    Admin {},
    /// Get's the currently nominated admin (if any).
    #[returns(crate::query::AdminNominationResponse)]
    AdminNomination {},
    /// Gets the contract's config.
    #[returns(Config)]
    Config {},
    /// Gets the token balance for each cw20 registered with the
    /// contract.
    #[returns(crate::query::Snip20BalanceResponse)]
    Cw20Balances {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Lists the addresses of the cw20 tokens in this contract's
    /// treasury.
    #[returns(Vec<cosmwasm_std::Addr>)]
    Cw20TokenList {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Lists the addresses of the cw721 tokens in this contract's
    /// treasury.
    #[returns(Vec<cosmwasm_std::Addr>)]
    Cw721TokenList {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Dumps all of the core contract's state in a single
    /// query. Useful for frontends as performance for queries is more
    /// limited by network times than compute times.
    #[returns(crate::query::DumpStateResponse)]
    DumpState {},
    /// Gets the address associated with an item key.
    #[returns(crate::query::GetItemResponse)]
    GetItem { key: String },
    /// Lists all of the items associted with the contract. For
    /// example, given the items `{ "group": "foo", "subdao": "bar"}`
    /// this query would return `[("group", "foo"), ("subdao",
    /// "bar")]`.
    #[returns(Vec<String>)]
    ListItems {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns contract version info
    #[returns(crate::voting::InfoResponse)]
    Info {},
    /// Gets all proposal modules associated with the
    /// contract.
    #[returns(Vec<crate::state::ProposalModule>)]
    ProposalModules {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Gets the active proposal modules associated with the
    /// contract.
    #[returns(Vec<crate::state::ProposalModule>)]
    ActiveProposalModules {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Gets the number of active and total proposal modules
    /// registered with this module.
    #[returns(crate::query::ProposalModuleCountResponse)]
    ProposalModuleCount {},
    /// Returns information about if the contract is currently paused.
    #[returns(crate::query::PauseInfoResponse)]
    PauseInfo {},
    /// Gets the contract's voting module.
    #[returns(cosmwasm_std::Addr)]
    VotingModule {},
    /// Returns all SubDAOs with their charters in a vec.
    /// start_after is bound exclusive and asks for a string address.
    #[returns(Vec<crate::query::SubDao>)]
    ListSubDaos {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Implements the DAO Star standard: <https://daostar.one/EIP>
    #[returns(crate::query::DaoURIResponse)]
    DaoURI {},
    /// Returns the voting power for an address at a given height.
    #[returns(crate::voting::VotingPowerAtHeightResponse)]
    VotingPowerAtHeight {
        address: String,
        height: Option<u64>,
    },
    /// Returns the total voting power at a given block height.
    #[returns(crate::voting::TotalPowerAtHeightResponse)]
    TotalPowerAtHeight { height: Option<u64> },
}

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MigrateMsg {
    FromV1 {
        dao_uri: Option<String>,
        params: Option<MigrateParams>,
    },
    FromCompatible {},
}
