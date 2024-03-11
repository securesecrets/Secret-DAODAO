#![allow(clippy::large_enum_variant)]

use std::fmt;

use cosmwasm_std::{Addr, Api, Binary, BlockInfo, CanonicalAddr, Coin, StdResult};
use schemars::JsonSchema;
use secret_toolkit::{permit::Permit, utils::HandleCallback};
use serde::{Deserialize, Serialize};

/// Instantiation message
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Snip721InstantiateMsg {
    /// name of token contract
    pub name: String,
    /// token contract symbol
    pub symbol: String,
    /// optional admin address, env.message.sender if missing
    pub admin: Option<String>,
    /// entropy used for prng seed
    pub entropy: String,
    /// optional royalty information to use as default when RoyaltyInfo is not provided to a
    /// minting function
    pub royalty_info: Option<RoyaltyInfo>,
    /// optional privacy configuration for the contract
    pub config: Option<InstantiateConfig>,
    /// optional callback message to execute after instantiation.  This will
    /// most often be used to have the token contract provide its address to a
    /// contract that instantiated it, but it could be used to execute any
    /// contract
    pub post_init_callback: Option<PostInstantiateCallback>,
}

/// This type represents optional configuration values.
/// All values are optional and have defaults which are more private by default,
/// but can be overridden if necessary
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct InstantiateConfig {
    /// indicates whether the token IDs and the number of tokens controlled by the contract are
    /// public.  If the token supply is private, only minters can view the token IDs and
    /// number of tokens controlled by the contract
    /// default: False
    pub public_token_supply: Option<bool>,
    /// indicates whether token ownership is public or private.  A user can still change whether the
    /// ownership of their tokens is public or private
    /// default: False
    pub public_owner: Option<bool>,
    /// indicates whether sealed metadata should be enabled.  If sealed metadata is enabled, the
    /// private metadata is not viewable by anyone, not even the owner, until the owner calls the
    /// Reveal function.  When Reveal is called, the sealed metadata is irreversibly moved to the
    /// public metadata (as default).  if unwrapped_metadata_is_private is set to true, it will
    /// remain as private metadata, but the owner will now be able to see it.  Anyone will be able
    /// to query the token to know that it has been unwrapped.  This simulates buying/selling a
    /// wrapped card that no one knows which card it is until it is unwrapped. If sealed metadata
    /// is not enabled, all tokens are considered unwrapped
    /// default:  False
    pub enable_sealed_metadata: Option<bool>,
    /// indicates if the Reveal function should keep the sealed metadata private after unwrapping
    /// This config value is ignored if sealed metadata is not enabled
    /// default: False
    pub unwrapped_metadata_is_private: Option<bool>,
    /// indicates whether a minter is permitted to update a token's metadata
    /// default: True
    pub minter_may_update_metadata: Option<bool>,
    /// indicates whether the owner of a token is permitted to update a token's metadata
    /// default: False
    pub owner_may_update_metadata: Option<bool>,
    /// Indicates whether burn functionality should be enabled
    /// default: False
    pub enable_burn: Option<bool>,
}

impl Default for InstantiateConfig {
    fn default() -> Self {
        InstantiateConfig {
            public_token_supply: Some(false),
            public_owner: Some(false),
            enable_sealed_metadata: Some(false),
            unwrapped_metadata_is_private: Some(false),
            minter_may_update_metadata: Some(true),
            owner_may_update_metadata: Some(false),
            enable_burn: Some(false),
        }
    }
}

/// info needed to perform a callback message after instantiation
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct PostInstantiateCallback {
    /// the callback message to execute
    pub msg: Binary,
    /// address of the contract to execute
    pub contract_address: String,
    /// code hash of the contract to execute
    pub code_hash: String,
    /// list of native Coin to send with the callback message
    pub send: Vec<Coin>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Snip721ExecuteMsg {
    /// mint new token
    MintNft {
        /// optional token id. if omitted, use current token index
        token_id: Option<String>,
        /// optional owner address. if omitted, owned by the message sender
        owner: Option<String>,
        /// optional public metadata that can be seen by everyone
        public_metadata: Option<Metadata>,
        /// optional private metadata that can only be seen by the owner and whitelist
        private_metadata: Option<Metadata>,
        /// optional serial number for this token
        serial_number: Option<SerialNumber>,
        /// optional royalty information for this token.  This will be ignored if the token is
        /// non-transferable
        royalty_info: Option<RoyaltyInfo>,
        /// optionally true if the token is transferable.  Defaults to true if omitted
        transferable: Option<bool>,
        /// optional memo for the tx
        memo: Option<String>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// Mint multiple tokens
    BatchMintNft {
        /// list of mint operations to perform
        mints: Vec<Mint>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// create a mint run of clones that will have MintRunInfos showing they are serialized
    /// copies in the same mint run with the specified quantity.  Mint_run_id can be used to
    /// track mint run numbers in subsequent MintNftClones calls.  So, if provided, the first
    /// MintNftClones call will have mint run number 1, the next time MintNftClones is called
    /// with the same mint_run_id, those clones will have mint run number 2, etc...  If no
    /// mint_run_id is specified, the clones will not have any mint run number assigned to their
    /// MintRunInfos.  Because this mints to a single address, there is no option to specify
    /// that the clones are non-transferable as there is no foreseen reason for someone to have
    /// multiple copies of an nft that they can never send to others
    MintNftClones {
        /// optional mint run ID
        mint_run_id: Option<String>,
        /// number of clones to mint
        quantity: u32,
        /// optional owner address. if omitted, owned by the message sender
        owner: Option<String>,
        /// optional public metadata that can be seen by everyone
        public_metadata: Option<Metadata>,
        /// optional private metadata that can only be seen by the owner and whitelist
        private_metadata: Option<Metadata>,
        /// optional royalty information for these tokens
        royalty_info: Option<RoyaltyInfo>,
        /// optional memo for the mint txs
        memo: Option<String>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// set the public and/or private metadata.  This can be called by either the token owner or
    /// a valid minter if they have been given this power by the appropriate config values
    SetMetadata {
        /// id of the token whose metadata should be updated
        token_id: String,
        /// the optional new public metadata
        public_metadata: Option<Metadata>,
        /// the optional new private metadata
        private_metadata: Option<Metadata>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// set royalty information.  If no token ID is provided, this royalty info will become the default
    /// RoyaltyInfo for any new tokens minted on the contract.  If a token ID is provided, this can only
    /// be called by the token creator and only when the creator is the current owner.  Royalties can not
    /// be set on a token that is not transferable, because they can never be sold
    SetRoyaltyInfo {
        /// optional id of the token whose royalty information should be updated.  If not provided,
        /// this updates the default royalty information for any new tokens minted on the contract
        token_id: Option<String>,
        /// the new royalty information.  If None, existing royalty information will be deleted.  It should
        /// be noted, that if deleting a token's royalty information while the contract has a default royalty
        /// info set up will give the token the default royalty information
        royalty_info: Option<RoyaltyInfo>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// Reveal the private metadata of a sealed token and mark the token as having been unwrapped
    Reveal {
        /// id of the token to unwrap
        token_id: String,
        /// optional message length padding
        padding: Option<String>,
    },
    /// if a contract was instantiated to make ownership public by default, this will allow
    /// an address to make the ownership of their tokens private.  The address can still use
    /// SetGlobalApproval to make ownership public either inventory-wide or for a specific token
    MakeOwnershipPrivate {
        /// optional message length padding
        padding: Option<String>,
    },
    /// add/remove approval(s) that whitelist everyone (makes public)
    SetGlobalApproval {
        /// optional token id to apply approval/revocation to
        token_id: Option<String>,
        /// optional permission level for viewing the owner
        view_owner: Option<AccessLevel>,
        /// optional permission level for viewing private metadata
        view_private_metadata: Option<AccessLevel>,
        /// optional expiration
        expires: Option<Expiration>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// add/remove approval(s) for a specific address on the token(s) you own.  Any permissions
    /// that are omitted will keep the current permission setting for that whitelist address
    SetWhitelistedApproval {
        /// address being granted/revoked permission
        address: String,
        /// optional token id to apply approval/revocation to
        token_id: Option<String>,
        /// optional permission level for viewing the owner
        view_owner: Option<AccessLevel>,
        /// optional permission level for viewing private metadata
        view_private_metadata: Option<AccessLevel>,
        /// optional permission level for transferring
        transfer: Option<AccessLevel>,
        /// optional expiration
        expires: Option<Expiration>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// gives the spender permission to transfer the specified token.  If you are the owner
    /// of the token, you can use SetWhitelistedApproval to accomplish the same thing.  If
    /// you are an operator, you can only use Approve
    Approve {
        /// address being granted the permission
        spender: String,
        /// id of the token that the spender can transfer
        token_id: String,
        /// optional expiration for this approval
        expires: Option<Expiration>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// revokes the spender's permission to transfer the specified token.  If you are the owner
    /// of the token, you can use SetWhitelistedApproval to accomplish the same thing.  If you
    /// are an operator, you can only use Revoke, but you can not revoke the transfer approval
    /// of another operator
    Revoke {
        /// address whose permission is revoked
        spender: String,
        /// id of the token that the spender can no longer transfer
        token_id: String,
        /// optional message length padding
        padding: Option<String>,
    },
    /// provided for cw721 compliance, but can be done with SetWhitelistedApproval...
    /// gives the operator permission to transfer all of the message sender's tokens
    ApproveAll {
        /// address being granted permission to transfer
        operator: String,
        /// optional expiration for this approval
        expires: Option<Expiration>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// provided for cw721 compliance, but can be done with SetWhitelistedApproval...
    /// revokes the operator's permission to transfer any of the message sender's tokens
    RevokeAll {
        /// address whose permissions are revoked
        operator: String,
        /// optional message length padding
        padding: Option<String>,
    },
    /// transfer a token if it is transferable
    TransferNft {
        /// recipient of the transfer
        recipient: String,
        /// id of the token to transfer
        token_id: String,
        /// optional memo for the tx
        memo: Option<String>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// transfer many tokens and fails if any are non-transferable
    BatchTransferNft {
        /// list of transfers to perform
        transfers: Vec<Transfer>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// send a token if it is transferable and call the receiving contract's (Batch)ReceiveNft
    SendNft {
        /// address to send the token to
        contract: String,
        /// optional code hash and BatchReceiveNft implementation status of the recipient contract
        receiver_info: Option<ReceiverInfo>,
        /// id of the token to send
        token_id: String,
        /// optional message to send with the (Batch)RecieveNft callback
        msg: Option<Binary>,
        /// optional memo for the tx
        memo: Option<String>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// send many tokens and call the receiving contracts' (Batch)ReceiveNft.  Fails if any tokens are
    /// non-transferable
    BatchSendNft {
        /// list of sends to perform
        sends: Vec<Send>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// burn a token.  This can be always be done on a non-transferable token, regardless of whether burn
    /// has been enabled on the contract.  An owner should always have a way to get rid of a token they do
    /// not want, and burning is the only way to do that if the token is non-transferable
    BurnNft {
        /// token to burn
        token_id: String,
        /// optional memo for the tx
        memo: Option<String>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// burn many tokens.  This can be always be done on a non-transferable token, regardless of whether burn
    /// has been enabled on the contract.  An owner should always have a way to get rid of a token they do
    /// not want, and burning is the only way to do that if the token is non-transferable
    BatchBurnNft {
        /// list of burns to perform
        burns: Vec<Burn>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// register that the message sending contract implements ReceiveNft and possibly
    /// BatchReceiveNft.  If a contract implements BatchReceiveNft, SendNft will always
    /// call BatchReceiveNft even if there is only one token transferred (the token_ids
    /// Vec will only contain one ID)
    RegisterReceiveNft {
        /// receving contract's code hash
        code_hash: String,
        /// optionally true if the contract also implements BatchReceiveNft.  Defaults
        /// to false if not specified
        also_implements_batch_receive_nft: Option<bool>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// create a viewing key
    CreateViewingKey {
        /// entropy String used in random key generation
        entropy: String,
        /// optional message length padding
        padding: Option<String>,
    },
    /// set viewing key
    SetViewingKey {
        /// desired viewing key
        key: String,
        /// optional message length padding
        padding: Option<String>,
    },
    /// add addresses with minting authority
    AddMinters {
        /// list of addresses that can now mint
        minters: Vec<String>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// revoke minting authority from addresses
    RemoveMinters {
        /// list of addresses no longer allowed to mint
        minters: Vec<String>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// define list of addresses with minting authority
    SetMinters {
        /// list of addresses with minting authority
        minters: Vec<String>,
        /// optional message length padding
        padding: Option<String>,
    },
    /// change address with administrative power
    ChangeAdmin {
        /// address with admin authority
        address: String,
        /// optional message length padding
        padding: Option<String>,
    },
    /// set contract status level to determine which functions are allowed.  StopTransactions
    /// status prevent mints, burns, sends, and transfers, but allows all other functions
    SetContractStatus {
        /// status level
        level: ContractStatus,
        /// optional message length padding
        padding: Option<String>,
    },
    /// disallow the use of a permit
    RevokePermit {
        /// name of the permit that is no longer valid
        permit_name: String,
        /// optional message length padding
        padding: Option<String>,
    },
}

impl HandleCallback for Snip721ExecuteMsg {
    const BLOCK_SIZE: usize = 256;
}

/// permission access level
#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AccessLevel {
    /// approve permission only for the specified token
    ApproveToken,
    /// grant permission for all tokens
    All,
    /// revoke permission only for the specified token
    RevokeToken,
    /// remove all permissions for this address
    None,
}

/// token mint info used when doing a BatchMint
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct Mint {
    /// optional token id, if omitted, use current token index
    pub token_id: Option<String>,
    /// optional owner address, owned by the minter otherwise
    pub owner: Option<String>,
    /// optional public metadata that can be seen by everyone
    pub public_metadata: Option<Metadata>,
    /// optional private metadata that can only be seen by owner and whitelist
    pub private_metadata: Option<Metadata>,
    /// optional serial number for this token
    pub serial_number: Option<SerialNumber>,
    /// optional royalty information for this token.  This will be ignored if the token is
    /// non-transferable
    pub royalty_info: Option<RoyaltyInfo>,
    /// optionally true if the token is transferable.  Defaults to true if omitted
    pub transferable: Option<bool>,
    /// optional memo for the tx
    pub memo: Option<String>,
}

/// token burn info used when doing a BatchBurnNft
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct Burn {
    /// tokens being burnt
    pub token_ids: Vec<String>,
    /// optional memo for the tx
    pub memo: Option<String>,
}

/// token transfer info used when doing a BatchTransferNft
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct Transfer {
    /// recipient of the transferred tokens
    pub recipient: String,
    /// tokens being transferred
    pub token_ids: Vec<String>,
    /// optional memo for the tx
    pub memo: Option<String>,
}

/// send token info used when doing a BatchSendNft
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct Send {
    /// recipient of the sent tokens
    pub contract: String,
    /// optional code hash and BatchReceiveNft implementation status of the recipient contract
    pub receiver_info: Option<ReceiverInfo>,
    /// tokens being sent
    pub token_ids: Vec<String>,
    /// optional message to send with the (Batch)RecieveNft callback
    pub msg: Option<Binary>,
    /// optional memo for the tx
    pub memo: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Snip721ExecuteAnswer {
    /// MintNft will also display the minted token's ID in the log attributes under the
    /// key `minted` in case minting was done as a callback message
    MintNft {
        token_id: String,
    },
    /// BatchMintNft will also display the minted tokens' IDs in the log attributes under the
    /// key `minted` in case minting was done as a callback message
    BatchMintNft {
        token_ids: Vec<String>,
    },
    /// Displays the token ids of the first minted NFT and the last minted NFT.  Because these
    /// are serialized clones, the ids of all the tokens minted in between should be easily
    /// inferred.  MintNftClones will also display the minted tokens' IDs in the log attributes
    /// under the keys `first_minted` and `last_minted` in case minting was done as a callback message
    MintNftClones {
        /// token id of the first minted clone
        first_minted: String,
        /// token id of the last minted clone
        last_minted: String,
    },
    SetMetadata {
        status: ResponseStatus,
    },
    SetRoyaltyInfo {
        status: ResponseStatus,
    },
    MakeOwnershipPrivate {
        status: ResponseStatus,
    },
    Reveal {
        status: ResponseStatus,
    },
    Approve {
        status: ResponseStatus,
    },
    Revoke {
        status: ResponseStatus,
    },
    ApproveAll {
        status: ResponseStatus,
    },
    RevokeAll {
        status: ResponseStatus,
    },
    SetGlobalApproval {
        status: ResponseStatus,
    },
    SetWhitelistedApproval {
        status: ResponseStatus,
    },
    TransferNft {
        status: ResponseStatus,
    },
    BatchTransferNft {
        status: ResponseStatus,
    },
    SendNft {
        status: ResponseStatus,
    },
    BatchSendNft {
        status: ResponseStatus,
    },
    BurnNft {
        status: ResponseStatus,
    },
    BatchBurnNft {
        status: ResponseStatus,
    },
    RegisterReceiveNft {
        status: ResponseStatus,
    },
    /// response from both setting and creating a viewing key
    ViewingKey {
        key: String,
    },
    AddMinters {
        status: ResponseStatus,
    },
    RemoveMinters {
        status: ResponseStatus,
    },
    SetMinters {
        status: ResponseStatus,
    },
    ChangeAdmin {
        status: ResponseStatus,
    },
    SetContractStatus {
        status: ResponseStatus,
    },
    RevokePermit {
        status: ResponseStatus,
    },
}

/// the address and viewing key making an authenticated query request
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct ViewerInfo {
    /// querying address
    pub address: String,
    /// authentication key string
    pub viewing_key: String,
}

/// a recipient contract's code hash and whether it implements BatchReceiveNft
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct ReceiverInfo {
    /// recipient's code hash
    pub recipient_code_hash: String,
    /// true if the contract also implements BacthReceiveNft.  Defaults to false
    /// if not specified
    pub also_implements_batch_receive_nft: Option<bool>,
}

/// tx type and specifics
#[derive(Serialize, Deserialize, JsonSchema, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TxAction {
    /// transferred token ownership
    Transfer {
        /// previous owner
        from: Addr,
        /// optional sender if not owner
        sender: Option<Addr>,
        /// new owner
        recipient: Addr,
    },
    /// minted new token
    Mint {
        /// minter's address
        minter: Addr,
        /// token's first owner
        recipient: Addr,
    },
    /// burned a token
    Burn {
        /// previous owner
        owner: Addr,
        /// burner's address if not owner
        burner: Option<Addr>,
    },
}

/// tx for display
#[derive(Serialize, Deserialize, JsonSchema, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Tx {
    /// tx id
    pub tx_id: u64,
    /// the block containing this tx
    pub block_height: u64,
    /// the time (in seconds since 01/01/1970) of the block containing this tx
    pub block_time: u64,
    /// token id
    pub token_id: String,
    /// tx type and specifics
    pub action: TxAction,
    /// optional memo
    pub memo: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Snip721QueryMsg {
    /// display the contract's name and symbol
    ContractInfo {},
    /// display the contract's configuration
    ContractConfig {},
    /// display the list of authorized minters
    Minters {},
    /// display the number of tokens controlled by the contract.  The token supply must
    /// either be public, or the querier must be an authenticated minter
    NumTokens {
        /// optional address and key requesting to view the number of tokens
        viewer: Option<ViewerInfo>,
    },
    /// display an optionally paginated list of all the tokens controlled by the contract.
    /// The token supply must either be public, or the querier must be an authenticated
    /// minter
    AllTokens {
        /// optional address and key requesting to view the list of tokens
        viewer: Option<ViewerInfo>,
        /// paginate by providing the last token_id received in the previous query
        start_after: Option<String>,
        /// optional number of token ids to display
        limit: Option<u32>,
    },
    /// display the owner of the specified token if authorized to view it.  If the requester
    /// is also the token's owner, the response will also include a list of any addresses
    /// that can transfer this token.  The transfer approval list is for CW721 compliance,
    /// but the NftDossier query will be more complete by showing viewing approvals as well
    OwnerOf {
        token_id: String,
        /// optional address and key requesting to view the token owner
        viewer: Option<ViewerInfo>,
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// displays the public metadata of a token
    NftInfo { token_id: String },
    /// displays all the information contained in the OwnerOf and NftInfo queries
    AllNftInfo {
        token_id: String,
        /// optional address and key requesting to view the token owner
        viewer: Option<ViewerInfo>,
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// displays the private metadata if permitted to view it
    PrivateMetadata {
        token_id: String,
        /// optional address and key requesting to view the private metadata
        viewer: Option<ViewerInfo>,
    },
    /// displays all the information about a token that the viewer has permission to
    /// see.  This may include the owner, the public metadata, the private metadata, royalty
    /// information, mint run information, whether the token is unwrapped, whether the token is
    /// transferable, and the token and inventory approvals
    NftDossier {
        token_id: String,
        /// optional address and key requesting to view the token information
        viewer: Option<ViewerInfo>,
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// displays all the information about multiple tokens that the viewer has permission to
    /// see.  This may include the owner, the public metadata, the private metadata, royalty
    /// information, mint run information, whether the token is unwrapped, whether the token is
    /// transferable, and the token and inventory approvals
    BatchNftDossier {
        token_ids: Vec<String>,
        /// optional address and key requesting to view the token information
        viewer: Option<ViewerInfo>,
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// list all the approvals in place for a specified token if given the owner's viewing
    /// key
    TokenApprovals {
        token_id: String,
        /// the token owner's viewing key
        viewing_key: String,
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// list all the inventory-wide approvals in place for the specified address if given the
    /// the correct viewing key for the address
    InventoryApprovals {
        address: String,
        /// the viewing key
        viewing_key: String,
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// displays a list of all the CW721-style operators (any address that was granted
    /// approval to transfer all of the owner's tokens).  This query is provided to maintain
    /// CW721 compliance, however, approvals are private on secret network, so only the
    /// owner's viewing key will authorize the ability to see the list of operators
    ApprovedForAll {
        owner: String,
        /// optional viewing key to authenticate this query.  It is "optional" only in the
        /// sense that a CW721 query does not have this field.  However, not providing the
        /// key will always result in an empty list
        viewing_key: Option<String>,
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// displays a list of all the tokens belonging to the input owner in which the viewer
    /// has view_owner permission
    Tokens {
        owner: String,
        /// optional address of the querier if different from the owner
        viewer: Option<String>,
        /// optional viewing key
        viewing_key: Option<String>,
        /// paginate by providing the last token_id received in the previous query
        start_after: Option<String>,
        /// optional number of token ids to display
        limit: Option<u32>,
    },
    /// displays the number of tokens that the querier has permission to see the owner and that
    /// belong to the specified address
    NumTokensOfOwner {
        owner: String,
        /// optional address of the querier if different from the owner
        viewer: Option<String>,
        /// optional viewing key
        viewing_key: Option<String>,
    },
    /// display if a token is unwrapped
    IsUnwrapped { token_id: String },
    /// display if a token is transferable
    IsTransferable { token_id: String },
    /// display that this contract implements non-transferable tokens
    ImplementsNonTransferableTokens {},
    /// display that this contract implements the use of the `token_subtype` metadata extension field
    ImplementsTokenSubtype {},
    /// verify that the specified address has approval to transfer every listed token.
    /// A token will count as unapproved if it is non-transferable
    VerifyTransferApproval {
        /// list of tokens to verify approval for
        token_ids: Vec<String>,
        /// address that has approval
        address: String,
        /// viewing key
        viewing_key: String,
    },
    /// display the transaction history for the specified address in reverse
    /// chronological order
    TransactionHistory {
        address: String,
        /// viewing key
        viewing_key: String,
        /// optional page to display
        page: Option<u32>,
        /// optional number of transactions per page
        page_size: Option<u32>,
    },
    /// display the code hash a contract has registered with the token contract and whether
    /// the contract implements BatchReceivenft
    RegisteredCodeHash {
        /// the contract whose receive registration info you want to view
        contract: String,
    },
    /// display the royalty information of a token if a token ID is specified, or display the
    /// contract's default royalty information in no token ID is provided
    RoyaltyInfo {
        /// optional ID of the token whose royalty information should be displayed.  If not
        /// provided, display the contract's default royalty information
        token_id: Option<String>,
        /// optional address and key requesting to view the royalty information
        viewer: Option<ViewerInfo>,
    },
    /// display the contract's creator
    ContractCreator {},
    /// perform queries by passing permits instead of viewing keys
    WithPermit {
        /// permit used to verify querier identity
        permit: Permit,
        /// query to perform
        query: QueryWithPermit,
    },
}

/// SNIP721 Approval
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Snip721Approval {
    /// whitelisted address
    pub address: Addr,
    /// optional expiration if the address has view owner permission
    pub view_owner_expiration: Option<Expiration>,
    /// optional expiration if the address has view private metadata permission
    pub view_private_metadata_expiration: Option<Expiration>,
    /// optional expiration if the address has transfer permission
    pub transfer_expiration: Option<Expiration>,
}

/// CW721 Approval
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Cw721Approval {
    /// address that can transfer the token
    pub spender: Addr,
    /// expiration of this approval
    pub expires: Expiration,
}

/// response of CW721 OwnerOf
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Cw721OwnerOfResponse {
    /// Owner of the token if permitted to view it
    pub owner: Option<Addr>,
    /// list of addresses approved to transfer this token
    pub approvals: Vec<Cw721Approval>,
}

/// the token id and nft dossier info of a single token response in a batch query
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct BatchNftDossierElement {
    pub token_id: String,
    pub owner: Option<Addr>,
    pub public_metadata: Option<Metadata>,
    pub private_metadata: Option<Metadata>,
    pub display_private_metadata_error: Option<String>,
    pub royalty_info: Option<DisplayRoyaltyInfo>,
    pub mint_run_info: Option<MintRunInfo>,
    /// true if this token is transferable
    pub transferable: bool,
    /// true if this token is unwrapped (returns true if the contract does not have selaed metadata enabled)
    pub unwrapped: bool,
    pub owner_is_public: bool,
    pub public_ownership_expiration: Option<Expiration>,
    pub private_metadata_is_public: bool,
    pub private_metadata_is_public_expiration: Option<Expiration>,
    pub token_approvals: Option<Vec<Snip721Approval>>,
    pub inventory_approvals: Option<Vec<Snip721Approval>>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Snip721QueryAnswer {
    ContractInfo {
        name: String,
        symbol: String,
    },
    ContractConfig {
        token_supply_is_public: bool,
        owner_is_public: bool,
        sealed_metadata_is_enabled: bool,
        unwrapped_metadata_is_private: bool,
        minter_may_update_metadata: bool,
        owner_may_update_metadata: bool,
        burn_is_enabled: bool,
        implements_non_transferable_tokens: bool,
        implements_token_subtype: bool,
    },
    Minters {
        minters: Vec<Addr>,
    },
    NumTokens {
        count: u32,
    },
    TokenList {
        tokens: Vec<String>,
    },
    OwnerOf {
        owner: Addr,
        approvals: Vec<Cw721Approval>,
    },
    TokenApprovals {
        owner_is_public: bool,
        public_ownership_expiration: Option<Expiration>,
        private_metadata_is_public: bool,
        private_metadata_is_public_expiration: Option<Expiration>,
        token_approvals: Vec<Snip721Approval>,
    },
    InventoryApprovals {
        owner_is_public: bool,
        public_ownership_expiration: Option<Expiration>,
        private_metadata_is_public: bool,
        private_metadata_is_public_expiration: Option<Expiration>,
        inventory_approvals: Vec<Snip721Approval>,
    },
    NftInfo {
        token_uri: Option<String>,
        extension: Option<Extension>,
    },
    PrivateMetadata {
        token_uri: Option<String>,
        extension: Option<Extension>,
    },
    AllNftInfo {
        access: Cw721OwnerOfResponse,
        info: Option<Metadata>,
    },
    NftDossier {
        owner: Option<Addr>,
        public_metadata: Option<Metadata>,
        private_metadata: Option<Metadata>,
        display_private_metadata_error: Option<String>,
        royalty_info: Option<DisplayRoyaltyInfo>,
        mint_run_info: Option<MintRunInfo>,
        transferable: bool,
        unwrapped: bool,
        owner_is_public: bool,
        public_ownership_expiration: Option<Expiration>,
        private_metadata_is_public: bool,
        private_metadata_is_public_expiration: Option<Expiration>,
        token_approvals: Option<Vec<Snip721Approval>>,
        inventory_approvals: Option<Vec<Snip721Approval>>,
    },
    BatchNftDossier {
        nft_dossiers: Vec<BatchNftDossierElement>,
    },
    ApprovedForAll {
        operators: Vec<Cw721Approval>,
    },
    IsUnwrapped {
        token_is_unwrapped: bool,
    },
    IsTransferable {
        token_is_transferable: bool,
    },
    ImplementsNonTransferableTokens {
        is_enabled: bool,
    },
    ImplementsTokenSubtype {
        is_enabled: bool,
    },
    VerifyTransferApproval {
        approved_for_all: bool,
        first_unapproved_token: Option<String>,
    },
    TransactionHistory {
        /// total transaction count
        total: u64,
        txs: Vec<Tx>,
    },
    RegisteredCodeHash {
        code_hash: Option<String>,
        also_implements_batch_receive_nft: bool,
    },
    RoyaltyInfo {
        royalty_info: Option<DisplayRoyaltyInfo>,
    },
    ContractCreator {
        creator: Option<Addr>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct NumTokens {
    pub count: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Success,
    Failure,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Normal,
    StopTransactions,
    StopAll,
}

impl ContractStatus {
    /// Returns u8 representation of the ContractStatus
    pub fn to_u8(&self) -> u8 {
        match self {
            ContractStatus::Normal => 0,
            ContractStatus::StopTransactions => 1,
            ContractStatus::StopAll => 2,
        }
    }
}

/// queries using permits instead of viewing keys
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryWithPermit {
    /// display the royalty information of a token if a token ID is specified, or display the
    /// contract's default royalty information in no token ID is provided
    RoyaltyInfo {
        /// optional ID of the token whose royalty information should be displayed.  If not
        /// provided, display the contract's default royalty information
        token_id: Option<String>,
    },
    /// displays the private metadata if permitted to view it
    PrivateMetadata { token_id: String },
    /// displays all the information about a token that the viewer has permission to
    /// see.  This may include the owner, the public metadata, the private metadata, royalty
    /// information, mint run information, whether the token is unwrapped, whether the token is
    /// transferable, and the token and inventory approvals
    NftDossier {
        token_id: String,
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// displays all the information about multiple tokens that the viewer has permission to
    /// see.  This may include the owner, the public metadata, the private metadata, royalty
    /// information, mint run information, whether the token is unwrapped, whether the token is
    /// transferable, and the token and inventory approvals
    BatchNftDossier {
        token_ids: Vec<String>,
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// display the owner of the specified token if authorized to view it.  If the requester
    /// is also the token's owner, the response will also include a list of any addresses
    /// that can transfer this token.  The transfer approval list is for CW721 compliance,
    /// but the NftDossier query will be more complete by showing viewing approvals as well
    OwnerOf {
        token_id: String,
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// displays all the information contained in the OwnerOf and NftInfo queries
    AllNftInfo {
        token_id: String,
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// list all the inventory-wide approvals in place for the permit creator
    InventoryApprovals {
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// verify that the permit creator has approval to transfer every listed token.
    /// A token will count as unapproved if it is non-transferable
    VerifyTransferApproval {
        /// list of tokens to verify approval for
        token_ids: Vec<String>,
    },
    /// display the transaction history for the permit creator in reverse
    /// chronological order
    TransactionHistory {
        /// optional page to display
        page: Option<u32>,
        /// optional number of transactions per page
        page_size: Option<u32>,
    },
    /// display the number of tokens controlled by the contract.  The token supply must
    /// either be public, or the querier must be an authenticated minter
    NumTokens {},
    /// display an optionally paginated list of all the tokens controlled by the contract.
    /// The token supply must either be public, or the querier must be an authenticated
    /// minter
    AllTokens {
        /// paginate by providing the last token_id received in the previous query
        start_after: Option<String>,
        /// optional number of token ids to display
        limit: Option<u32>,
    },
    /// list all the approvals in place for a specified token if given the owner's permit
    TokenApprovals {
        token_id: String,
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// displays a list of all the CW721-style operators (any address that was granted
    /// approval to transfer all of the owner's tokens).  This query is provided to maintain
    /// CW721 compliance
    ApprovedForAll {
        /// optionally include expired Approvals in the response list.  If ommitted or
        /// false, expired Approvals will be filtered out of the response
        include_expired: Option<bool>,
    },
    /// displays a list of all the tokens belonging to the input owner in which the permit
    /// creator has view_owner permission
    Tokens {
        owner: String,
        /// paginate by providing the last token_id received in the previous query
        start_after: Option<String>,
        /// optional number of token ids to display
        limit: Option<u32>,
    },
    /// displays the number of tokens that the querier has permission to see the owner and that
    /// belong to the specified address
    NumTokensOfOwner { owner: String },
}

/// information about the minting of the NFT
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct MintRunInfo {
    /// optional address of the SNIP-721 contract creator
    pub collection_creator: Option<Addr>,
    /// optional address of this NFT's creator
    pub token_creator: Option<Addr>,
    /// optional time of minting (in seconds since 01/01/1970)
    pub time_of_minting: Option<u64>,
    /// optional number of the mint run this token was minted in.  A mint run represents a
    /// batch of NFTs released at the same time.  So if a creator decided to make 100 copies
    /// of an NFT, they would all be part of mint run number 1.  If they sold quickly, and
    /// the creator wanted to rerelease that NFT, he could make 100 more copies which would all
    /// be part of mint run number 2.
    pub mint_run: Option<u32>,
    /// optional serial number in this mint run.  This is used to serialize
    /// identical NFTs
    pub serial_number: Option<u32>,
    /// optional total number of NFTs minted on this run.  This is used to
    /// represent that this token is number m of n
    pub quantity_minted_this_run: Option<u32>,
}

/// Serial number to give an NFT when minting
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct SerialNumber {
    /// optional number of the mint run this token will be minted in.  A mint run represents a
    /// batch of NFTs released at the same time.  So if a creator decided to make 100 copies
    /// of an NFT, they would all be part of mint run number 1.  If they sold quickly, and
    /// the creator wanted to rerelease that NFT, he could make 100 more copies which would all
    /// be part of mint run number 2.
    pub mint_run: Option<u32>,
    /// serial number (in this mint run).  This is used to serialize
    /// identical NFTs
    pub serial_number: u32,
    /// optional total number of NFTs minted on this run.  This is used to
    /// represent that this token is number m of n
    pub quantity_minted_this_run: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
/// at the given point in time and after, Expiration will be considered expired
pub enum Expiration {
    /// expires at this block height
    AtHeight(u64),
    /// expires at the time in seconds since 01/01/1970
    AtTime(u64),
    /// never expires
    Never,
}

impl fmt::Display for Expiration {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expiration::AtHeight(height) => write!(f, "expiration height: {}", height),
            Expiration::AtTime(time) => write!(f, "expiration time: {}", time),
            Expiration::Never => write!(f, "expiration: never"),
        }
    }
}

/// default is Never
impl Default for Expiration {
    fn default() -> Self {
        Expiration::Never
    }
}

impl Expiration {
    /// Returns bool, true if Expiration has expired
    ///
    /// # Arguments
    ///
    /// * `block` - a reference to the BlockInfo containing the time to compare the Expiration to
    pub fn is_expired(&self, block: &BlockInfo) -> bool {
        match self {
            Expiration::AtHeight(height) => block.height >= *height,
            Expiration::AtTime(time) => block.time.seconds() >= *time,
            Expiration::Never => false,
        }
    }
}

/// data for a single royalty
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct Royalty {
    /// address to send royalties to
    pub recipient: String,
    /// royalty rate
    pub rate: u16,
}

impl Royalty {
    /// Returns StdResult<StoredRoyalty> from creating a StoredRoyalty from a Royalty
    ///
    /// # Arguments
    ///
    /// * `api` - a reference to the Api used to convert human and canonical addresses
    pub fn to_stored(&self, api: &dyn Api) -> StdResult<StoredRoyalty> {
        Ok(StoredRoyalty {
            recipient: api.addr_canonicalize(api.addr_validate(&self.recipient)?.as_str())?,
            rate: self.rate,
        })
    }
}

/// all royalty information
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct RoyaltyInfo {
    /// decimal places in royalty rates
    pub decimal_places_in_rates: u8,
    /// list of royalties
    pub royalties: Vec<Royalty>,
}

impl RoyaltyInfo {
    /// Returns StdResult<StoredRoyaltyInfo> from creating a StoredRoyaltyInfo from a RoyaltyInfo
    ///
    /// # Arguments
    ///
    /// * `api` - a reference to the Api used to convert human and canonical addresses
    pub fn to_stored(&self, api: &dyn Api) -> StdResult<StoredRoyaltyInfo> {
        Ok(StoredRoyaltyInfo {
            decimal_places_in_rates: self.decimal_places_in_rates,
            royalties: self
                .royalties
                .iter()
                .map(|r| r.to_stored(api))
                .collect::<StdResult<Vec<StoredRoyalty>>>()?,
        })
    }
}

/// display for a single royalty
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct DisplayRoyalty {
    /// address to send royalties to.  Can be None to keep addresses private
    pub recipient: Option<Addr>,
    /// royalty rate
    pub rate: u16,
}

/// display all royalty information
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct DisplayRoyaltyInfo {
    /// decimal places in royalty rates
    pub decimal_places_in_rates: u8,
    /// list of royalties
    pub royalties: Vec<DisplayRoyalty>,
}

/// data for storing a single royalty
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct StoredRoyalty {
    /// address to send royalties to
    pub recipient: CanonicalAddr,
    /// royalty rate
    pub rate: u16,
}

impl StoredRoyalty {
    /// Returns StdResult<DisplayRoyalty> from creating a DisplayRoyalty from a StoredRoyalty
    ///
    /// # Arguments
    ///
    /// * `api` - a reference to the Api used to convert human and canonical addresses
    /// * `hide_addr` - true if the address should be kept hidden
    pub fn to_human(&self, api: &dyn Api, hide_addr: bool) -> StdResult<DisplayRoyalty> {
        let recipient = if hide_addr {
            None
        } else {
            Some(api.addr_humanize(&self.recipient)?)
        };
        Ok(DisplayRoyalty {
            recipient,
            rate: self.rate,
        })
    }
}

/// all stored royalty information
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct StoredRoyaltyInfo {
    /// decimal places in royalty rates
    pub decimal_places_in_rates: u8,
    /// list of royalties
    pub royalties: Vec<StoredRoyalty>,
}

impl StoredRoyaltyInfo {
    /// Returns StdResult<DisplayRoyaltyInfo> from creating a DisplayRoyaltyInfo from a StoredRoyaltyInfo
    ///
    /// # Arguments
    ///
    /// * `api` - a reference to the Api used to convert human and canonical addresses
    /// * `hide_addr` - true if the address should be kept hidden
    pub fn to_human(&self, api: &dyn Api, hide_addr: bool) -> StdResult<DisplayRoyaltyInfo> {
        Ok(DisplayRoyaltyInfo {
            decimal_places_in_rates: self.decimal_places_in_rates,
            royalties: self
                .royalties
                .iter()
                .map(|r| r.to_human(api, hide_addr))
                .collect::<StdResult<Vec<DisplayRoyalty>>>()?,
        })
    }
}

/// token
#[derive(Serialize, Deserialize)]
pub struct Token {
    /// owner
    pub owner: CanonicalAddr,
    /// permissions granted for this token
    pub permissions: Vec<Permission>,
    /// true if this token has been unwrapped.  If sealed metadata is not enabled, all
    /// tokens are considered unwrapped
    pub unwrapped: bool,
    /// true if this token is transferable
    pub transferable: bool,
}

/// token metadata
#[derive(Serialize, Deserialize, JsonSchema, Clone, PartialEq, Eq, Debug, Default)]
pub struct Metadata {
    /// optional uri for off-chain metadata.  This should be prefixed with `http://`, `https://`, `ipfs://`, or
    /// `ar://`.  Only use this if you are not using `extension`
    pub token_uri: Option<String>,
    /// optional on-chain metadata.  Only use this if you are not using `token_uri`
    pub extension: Option<Extension>,
}

/// metadata extension
/// You can add any metadata fields you need here.  These fields are based on
/// https://docs.opensea.io/docs/metadata-standards and are the metadata fields that
/// Stashh uses for robust NFT display.  Urls should be prefixed with `http://`, `https://`, `ipfs://`, or
/// `ar://`
#[derive(Serialize, Deserialize, JsonSchema, Clone, PartialEq, Eq, Debug, Default)]
pub struct Extension {
    /// url to the image
    pub image: Option<String>,
    /// raw SVG image data (not recommended). Only use this if you're not including the image parameter
    pub image_data: Option<String>,
    /// url to allow users to view the item on your site
    pub external_url: Option<String>,
    /// item description
    pub description: Option<String>,
    /// name of the item
    pub name: Option<String>,
    /// item attributes
    pub attributes: Option<Vec<Trait>>,
    /// background color represented as a six-character hexadecimal without a pre-pended #
    pub background_color: Option<String>,
    /// url to a multimedia attachment
    pub animation_url: Option<String>,
    /// url to a YouTube video
    pub youtube_url: Option<String>,
    /// media files as specified on Stashh that allows for basic authenticatiion and decryption keys.
    /// Most of the above is used for bridging public eth NFT metadata easily, whereas `media` will be used
    /// when minting NFTs on Stashh
    pub media: Option<Vec<MediaFile>>,
    /// a select list of trait_types that are in the private metadata.  This will only ever be used
    /// in public metadata
    pub protected_attributes: Option<Vec<String>>,
    /// token subtypes used by Stashh for display groupings (primarily used for badges, which are specified
    /// by using "badge" as the token_subtype)
    pub token_subtype: Option<String>,
}

/// attribute trait
#[derive(Serialize, Deserialize, JsonSchema, Clone, PartialEq, Eq, Debug, Default)]
pub struct Trait {
    /// indicates how a trait should be displayed
    pub display_type: Option<String>,
    /// name of the trait
    pub trait_type: Option<String>,
    /// trait value
    pub value: String,
    /// optional max value for numerical traits
    pub max_value: Option<String>,
}

/// media file
#[derive(Serialize, Deserialize, JsonSchema, Clone, PartialEq, Eq, Debug, Default)]
pub struct MediaFile {
    /// file type
    /// Stashh currently uses: "image", "video", "audio", "text", "font", "application"
    pub file_type: Option<String>,
    /// file extension
    pub extension: Option<String>,
    /// authentication information
    pub authentication: Option<Authentication>,
    /// url to the file.  Urls should be prefixed with `http://`, `https://`, `ipfs://`, or `ar://`
    pub url: String,
}

/// media file authentication
#[derive(Serialize, Deserialize, JsonSchema, Clone, PartialEq, Eq, Debug, Default)]
pub struct Authentication {
    /// either a decryption key for encrypted files or a password for basic authentication
    pub key: Option<String>,
    /// username used in basic authentication
    pub user: Option<String>,
}

/// permission to view token info/transfer tokens
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Permission {
    /// permitted address
    pub address: CanonicalAddr,
    /// list of permission expirations for this address
    pub expirations: [Option<Expiration>; 3],
}
