use cosmwasm_std::Binary;
use dao_snip721_extensions::roles::{ExecuteExt, MetadataExt};
use schemars::JsonSchema;
use secret_toolkit::utils::{HandleCallback, InitCallback};
use serde::{Deserialize, Serialize};
use shade_protocol::utils::asset::RawContract;
use snip721_roles_impl::{
    expiration::Expiration,
    msg::{
        AccessLevel, Burn, ContractStatus, InstantiateConfig, Mint, PostInstantiateCallback,
        ReceiverInfo, Send, Transfer,
    },
    royalties::RoyaltyInfo,
};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Snip721RolesInstantiateMsg {
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
    pub query_auth: RawContract,
}

impl InitCallback for Snip721RolesInstantiateMsg {
    const BLOCK_SIZE: usize = 264;
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Snip721RolesExecuteMsg<MetadataExt, ExecuteExt> {
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
        /// Any custom extension used by this contract
        extension: MetadataExt,
    },
    /// Mint multiple tokens
    BatchMintNft {
        /// list of mint operations to perform
        mints: Vec<Mint<MetadataExt>>,
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
        /// Any custom extension used by this contract
        extension: MetadataExt,
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
    /// Extension msg
    Extension { msg: ExecuteExt },
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

impl HandleCallback for Snip721RolesExecuteMsg<MetadataExt, ExecuteExt> {
    const BLOCK_SIZE: usize = 264;
}
