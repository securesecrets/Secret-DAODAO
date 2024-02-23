use cosmwasm_schema::schemars::JsonSchema;
use cosmwasm_std::{
    to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, SubMsg, WasmMsg,
};

use cw_hooks::HookItem;
use secret_cw2::set_contract_version;

use cw_denom::UncheckedDenom;
use dao_interface::voting::{Query as CwCoreQuery, VotingPowerAtHeightResponse};
use dao_voting::{
    deposit::{DepositRefundPolicy, UncheckedDepositInfo},
    status::Status,
};
use serde::Serialize;

use crate::{
    error::PreProposeError,
    msg::{DepositInfoResponse, ExecuteMsg, InstantiateMsg, QueryMsg},
    state::{Config, PreProposeContract},
};

const CONTRACT_NAME: &str = "crates.io::dao-pre-propose-base";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

impl<InstantiateExt, ExecuteExt, QueryExt, ProposalMessage>
    PreProposeContract<InstantiateExt, ExecuteExt, QueryExt, ProposalMessage>
where
    ProposalMessage: Serialize,
    QueryExt: JsonSchema,
{
    pub fn instantiate(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: InstantiateMsg<InstantiateExt>,
    ) -> Result<Response, PreProposeError> {
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

        // The proposal module instantiates us. We're
        // making limited assumptions here. The only way to associate
        // a deposit module with a proposal module is for the proposal
        // module to instantiate it.
        self.proposal_module.save(deps.storage, &info.sender)?;

        // Query the proposal module for its DAO.
        let dao: Addr = deps.querier.query_wasm_smart(
            env.contract.code_hash.clone(),
            info.sender.clone(),
            &CwCoreQuery::Dao {},
        )?;

        self.dao.save(deps.storage, &dao)?;

        let deposit_info = msg
            .deposit_info
            .map(|info| {
                info.into_checked(deps.as_ref(), dao.clone(), env.contract.code_hash.clone())
            })
            .transpose()?;

        let config = Config {
            deposit_info,
            open_proposal_submission: msg.open_proposal_submission,
        };

        self.config.save(deps.storage, &config)?;

        Ok(Response::default()
            .add_attribute("method", "instantiate")
            .add_attribute("proposal_module", info.sender.into_string())
            .add_attribute("deposit_info", format!("{:?}", config.deposit_info))
            .add_attribute(
                "open_proposal_submission",
                config.open_proposal_submission.to_string(),
            )
            .add_attribute("dao", dao))
    }

    pub fn execute(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: ExecuteMsg<ProposalMessage, ExecuteExt>,
    ) -> Result<Response, PreProposeError> {
        match msg {
            ExecuteMsg::Propose { msg } => self.execute_propose(deps, env, info, msg),
            ExecuteMsg::UpdateConfig {
                deposit_info,
                open_proposal_submission,
            } => {
                self.execute_update_config(deps, info, env, deposit_info, open_proposal_submission)
            }
            ExecuteMsg::Withdraw { denom, key } => {
                self.execute_withdraw(deps.as_ref(), env, info, denom, key)
            }
            ExecuteMsg::AddProposalSubmittedHook { address,code_hash } => {
                self.execute_add_proposal_submitted_hook(deps, info, address,code_hash)
            }
            ExecuteMsg::RemoveProposalSubmittedHook { address,code_hash } => {
                self.execute_remove_proposal_submitted_hook(deps, info, address,code_hash)
            }
            ExecuteMsg::ProposalCompletedHook {
                proposal_id,
                new_status,
            } => self.execute_proposal_completed_hook(
                deps.as_ref(),
                info,
                env,
                proposal_id,
                new_status,
            ),

            ExecuteMsg::Extension { .. } => Ok(Response::default()),
        }
    }

    pub fn execute_propose(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: ProposalMessage,
    ) -> Result<Response, PreProposeError> {
        self.check_can_submit(
            deps.as_ref(),
            info.sender.clone(),
            env.contract.code_hash.clone(),
        )?;

        let config = self.config.load(deps.storage)?;

        let deposit_messages = if let Some(ref deposit_info) = config.deposit_info {
            deposit_info.check_native_deposit_paid(&info)?;
            deposit_info.get_take_deposit_messages(
                env.contract.code_hash.clone(),
                &info.sender,
                &env.contract.address.clone(),
            )?
        } else {
            vec![]
        };

        let proposal_module = self.proposal_module.load(deps.storage)?;

        // Snapshot the deposit using the ID of the proposal that we
        // will create.
        let next_id = deps.querier.query_wasm_smart(
            env.contract.code_hash.clone(),
            &proposal_module,
            &dao_interface::proposal::Query::NextProposalId {},
        )?;
        self.deposits.insert(
            deps.storage,
            &next_id,
            &(config.deposit_info, info.sender.clone()),
        )?;

        let propose_messsage = WasmMsg::Execute {
            contract_addr: proposal_module.into_string(),
            code_hash: env.contract.code_hash.clone(),
            msg: to_binary(&msg)?,
            funds: vec![],
        };

        let hooks_msgs = self
            .proposal_submitted_hooks
            .prepare_hooks(deps.storage, |hook_item| {
                let execute = WasmMsg::Execute {
                    contract_addr: hook_item.addr.into_string(),
                    code_hash: hook_item.code_hash.clone(),
                    msg: to_binary(&msg)?,
                    funds: vec![],
                };
                Ok(SubMsg::new(execute))
            })?;

        Ok(Response::default()
            .add_attribute("method", "execute_propose")
            .add_attribute("sender", info.sender)
            // It's important that the propose message is
            // first. Otherwise, a hook receiver could create a
            // proposal before us and invalidate our `NextProposalId
            // {}` query.
            .add_message(propose_messsage)
            .add_submessages(hooks_msgs)
            .add_messages(deposit_messages))
    }

    pub fn execute_update_config(
        &self,
        deps: DepsMut,
        info: MessageInfo,
        env: Env,
        deposit_info: Option<UncheckedDepositInfo>,
        open_proposal_submission: bool,
    ) -> Result<Response, PreProposeError> {
        let dao = self.dao.load(deps.storage)?;
        if info.sender != dao {
            Err(PreProposeError::NotDao {})
        } else {
            let deposit_info = deposit_info
                .map(|d| d.into_checked(deps.as_ref(), dao, env.contract.code_hash.clone()))
                .transpose()?;
            self.config.save(
                deps.storage,
                &Config {
                    deposit_info,
                    open_proposal_submission,
                },
            )?;

            Ok(Response::default()
                .add_attribute("method", "update_config")
                .add_attribute("sender", info.sender))
        }
    }

    pub fn execute_withdraw(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
        denom: Option<UncheckedDenom>,
        key: String,
    ) -> Result<Response, PreProposeError> {
        let dao = self.dao.load(deps.storage)?;
        if info.sender != dao {
            Err(PreProposeError::NotDao {})
        } else {
            let denom = match denom {
                Some(denom) => Some(denom.into_checked(deps, env.contract.code_hash.clone())?),
                None => {
                    let config = self.config.load(deps.storage)?;
                    config.deposit_info.map(|d| d.denom)
                }
            };
            match denom {
                None => Err(PreProposeError::NoWithdrawalDenom {}),
                Some(denom) => {
                    let balance = denom.query_balance(
                        &deps.querier,
                        env.contract.code_hash.clone(),
                        &env.contract.address,
                        key,
                    )?;
                    if balance.is_zero() {
                        Err(PreProposeError::NothingToWithdraw {})
                    } else {
                        let withdraw_message = denom.get_transfer_to_message(
                            env.contract.code_hash.clone(),
                            &dao,
                            balance,
                        )?;
                        Ok(Response::default()
                            .add_message(withdraw_message)
                            .add_attribute("method", "withdraw")
                            .add_attribute("receiver", &dao)
                            .add_attribute("denom", denom.to_string()))
                    }
                }
            }
        }
    }

    pub fn execute_add_proposal_submitted_hook(
        &self,
        deps: DepsMut,
        info: MessageInfo,
        address: String,
        code_hash: String
    ) -> Result<Response, PreProposeError> {
        let dao = self.dao.load(deps.storage)?;
        if info.sender != dao {
            return Err(PreProposeError::NotDao {});
        }

        let addr = deps.api.addr_validate(&address)?;
        self.proposal_submitted_hooks.add_hook(deps.storage, HookItem{
            addr,
            code_hash,
        })?;

        Ok(Response::default())
    }

    pub fn execute_remove_proposal_submitted_hook(
        &self,
        deps: DepsMut,
        info: MessageInfo,
        address: String,
        code_hash: String
    ) -> Result<Response, PreProposeError> {
        let dao = self.dao.load(deps.storage)?;
        if info.sender != dao {
            return Err(PreProposeError::NotDao {});
        }

        // Validate address
        let addr = deps.api.addr_validate(&address)?;

        // Remove the hook
        self.proposal_submitted_hooks
            .remove_hook(deps.storage, HookItem{
                addr,
                code_hash,
            })?;

        Ok(Response::default())
    }

    pub fn execute_proposal_completed_hook(
        &self,
        deps: Deps,
        info: MessageInfo,
        env: Env,
        id: u64,
        new_status: Status,
    ) -> Result<Response, PreProposeError> {
        let proposal_module = self.proposal_module.load(deps.storage)?;
        if info.sender != proposal_module {
            return Err(PreProposeError::NotModule {});
        }

        // If we receive a proposal completed hook from a proposal
        // module, and it is not in one of these states, something
        // bizare has happened. In that event, this message errors
        // which ought to cause the proposal module to remove this
        // module and open proposal submission to anyone.
        if new_status != Status::Closed
            && new_status != Status::Executed
            && new_status != Status::Vetoed
        {
            return Err(PreProposeError::NotCompleted { status: new_status });
        }

        match self.deposits.get(deps.storage, &id) {
            Some((deposit_info, proposer)) => {
                let messages = if let Some(ref deposit_info) = deposit_info {
                    // Determine if refund can be issued
                    let should_refund_to_proposer =
                        match (new_status, deposit_info.clone().refund_policy) {
                            // If policy is refund only passed props, refund for executed status
                            (Status::Executed, DepositRefundPolicy::OnlyPassed) => true,
                            // Don't refund other statuses for OnlyPassed policy
                            (_, DepositRefundPolicy::OnlyPassed) => false,
                            // Refund if the refund policy is always refund
                            (_, DepositRefundPolicy::Always) => true,
                            // Don't refund if the refund is never refund
                            (_, DepositRefundPolicy::Never) => false,
                        };

                    if should_refund_to_proposer {
                        deposit_info
                            .get_return_deposit_message(&proposer, env.contract.code_hash.clone())?
                    } else {
                        // If the proposer doesn't get the deposit, the DAO does.
                        let dao = self.dao.load(deps.storage)?;
                        deposit_info
                            .get_return_deposit_message(&dao, env.contract.code_hash.clone())?
                    }
                } else {
                    // No deposit info for this proposal. Nothing to do.
                    vec![]
                };

                Ok(Response::default()
                    .add_attribute("method", "execute_proposal_completed_hook")
                    .add_attribute("proposal", id.to_string())
                    .add_attribute("deposit_info", to_binary(&deposit_info)?.to_string())
                    .add_messages(messages))
            }

            // If we do not have a deposit for this proposal it was
            // likely created before we were added to the proposal
            // module. In that case, it's not our problem and we just
            // do nothing.
            None => Ok(Response::default()
                .add_attribute("method", "execute_proposal_completed_hook")
                .add_attribute("proposal", id.to_string())),
        }
    }

    pub fn check_can_submit(
        &self,
        deps: Deps,
        who: Addr,
        code_hash: String,
    ) -> Result<(), PreProposeError> {
        let config = self.config.load(deps.storage)?;

        if !config.open_proposal_submission {
            let dao = self.dao.load(deps.storage)?;
            let voting_power: VotingPowerAtHeightResponse = deps.querier.query_wasm_smart(
                code_hash,
                dao.into_string(),
                &CwCoreQuery::VotingPowerAtHeight {
                    address: who.into_string(),
                    height: None,
                },
            )?;
            if voting_power.power.is_zero() {
                return Err(PreProposeError::NotMember {});
            }
        }
        Ok(())
    }

    pub fn query(&self, deps: Deps, _env: Env, msg: QueryMsg<QueryExt>) -> StdResult<Binary> {
        match msg {
            QueryMsg::ProposalModule {} => to_binary(&self.proposal_module.load(deps.storage)?),
            QueryMsg::Dao {} => to_binary(&self.dao.load(deps.storage)?),
            QueryMsg::Config {} => to_binary(&self.config.load(deps.storage)?),
            QueryMsg::DepositInfo { proposal_id } => {
                let (deposit_info, proposer) =
                    self.deposits.get(deps.storage, &proposal_id).unwrap();
                to_binary(&DepositInfoResponse {
                    deposit_info,
                    proposer,
                })
            }
            QueryMsg::ProposalSubmittedHooks {} => {
                to_binary(&self.proposal_submitted_hooks.query_hooks(deps)?)
            }
            QueryMsg::QueryExtension { .. } => Ok(Binary::default()),
        }
    }
}
