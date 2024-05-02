use dao_snip721_extensions::roles::{ExecuteExt, MetadataExt, QueryExt};

pub type InstantiateMsg = snip721_roles_impl::msg::InstantiateMsg;
pub type ExecuteMsg = snip721_roles_impl::msg::ExecuteMsg<MetadataExt, ExecuteExt>;
pub type QueryMsg = snip721_roles_impl::msg::QueryMsg<QueryExt>;
