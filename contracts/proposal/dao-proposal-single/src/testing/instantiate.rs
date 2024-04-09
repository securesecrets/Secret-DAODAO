use cosmwasm_std::{to_binary, Addr, Coin, ContractInfo, Decimal, Empty, Uint128};

use dao_interface::state::{Admin, AnyContractInfo, ModuleInstantiateInfo};
use dao_pre_propose_single as cppbps;
use secret_multi_test::{next_block, App, BankSudo, ContractInstantiationInfo, Executor, SudoMsg};
use secret_utils::Duration;

use dao_voting::{
    deposit::UncheckedDepositInfo,
    pre_propose::PreProposeInfo,
    threshold::{ActiveThreshold, PercentageThreshold, Threshold::ThresholdQuorum},
};
use dao_voting_cw4::msg::GroupContract;
use dao_voting_snip20_staked::snip20_msg::InitialBalance as InitialBalanceSnip20Staked;
use shade_protocol::utils::asset::RawContract;
use snip20_reference_impl::msg::InitialBalance;
use snip721_reference_impl::msg::ReceiverInfo;

use crate::msg::InstantiateMsg;

use super::{
    contracts::{
        cw4_group_contract, cw4_voting_contract, cw_core_contract,
        native_staked_balances_voting_contract, proposal_single_contract, query_auth_contract,
        snip20_base_contract, snip20_stake_contract, snip20_staked_balances_voting_contract,
        snip721_base_contract, snip721_stake_contract,
    },
    CREATOR_ADDR,
};

pub(crate) fn get_pre_propose_info(
    app: &mut App,
    deposit_info: Option<UncheckedDepositInfo>,
    open_proposal_submission: bool,
    proposal_module_code_hash: String,
) -> PreProposeInfo {
    let pre_propose_contract =
        app.store_code(crate::testing::contracts::pre_propose_single_contract());
    PreProposeInfo::ModuleMayPropose {
        info: ModuleInstantiateInfo {
            code_id: pre_propose_contract.code_id,
            code_hash: pre_propose_contract.code_hash,
            msg: to_binary(&cppbps::InstantiateMsg {
                deposit_info,
                open_proposal_submission,
                extension: Empty::default(),
                proposal_module_code_hash,
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "pre_propose_contract".to_string(),
        },
    }
}

pub(crate) fn get_default_token_dao_proposal_module_instantiate(
    query_auth: RawContract,
    dao_code_hash: String,
) -> InstantiateMsg {
    InstantiateMsg {
        veto: None,
        threshold: ThresholdQuorum {
            quorum: PercentageThreshold::Percent(Decimal::percent(15)),
            threshold: PercentageThreshold::Majority {},
        },
        max_voting_period: Duration::Time(604800), // One week.
        min_voting_period: None,
        only_members_execute: true,
        allow_revoting: false,
        pre_propose_info: PreProposeInfo::AnyoneMayPropose {},
        close_proposal_on_execution_failure: true,
        dao_code_hash,
        query_auth,
    }
}

// Same as above but no proposal deposit.
pub(crate) fn _get_default_non_token_dao_proposal_module_instantiate(
    app: &mut App,
    proposal_module_code_hash: String,
    query_auth: RawContract,
    dao_code_hash: String,
) -> InstantiateMsg {
    InstantiateMsg {
        veto: None,
        threshold: ThresholdQuorum {
            threshold: PercentageThreshold::Percent(Decimal::percent(15)),
            quorum: PercentageThreshold::Majority {},
        },
        max_voting_period: Duration::Time(604800), // One week.
        min_voting_period: None,
        only_members_execute: true,
        allow_revoting: false,
        pre_propose_info: get_pre_propose_info(app, None, false, proposal_module_code_hash),
        close_proposal_on_execution_failure: true,
        dao_code_hash,
        query_auth,
    }
}

pub(crate) fn _instantiate_with_staked_snip721_governance(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_balances: Option<Vec<InitialBalance>>,
    dao_code_hash: String,
    query_auth: RawContract,
) -> ContractInfo {
    let proposal_module_info = app.store_code(proposal_single_contract());

    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    let initial_balances: Vec<InitialBalance> = {
        let mut already_seen = vec![];
        initial_balances
            .into_iter()
            .filter(|InitialBalance { address, amount: _ }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .collect()
    };

    let snip721_info = app.store_code(snip721_base_contract());
    let snip721_stake_info = app.store_code(snip721_stake_contract());
    let core_contract_info = app.store_code(cw_core_contract());

    let nft_info = app
        .instantiate_contract(
            snip721_info,
            Addr::unchecked("ekez"),
            &snip721_reference_impl::msg::InstantiateMsg {
                admin: Some("ekez".to_string()),
                symbol: "token".to_string(),
                name: "ekez token best token".to_string(),
                entropy: "entropy".to_string(),
                royalty_info: None,
                config: None,
                post_init_callback: None,
            },
            &[],
            "nft-staking",
            None,
        )
        .unwrap();

    let instantiate_core = dao_interface::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        dao_uri: None,
        image_url: None,
        automatically_add_snip20s: true,
        automatically_add_snip721s: false,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: snip721_stake_info.code_id,
            code_hash: snip721_stake_info.code_hash,
            msg: to_binary(&dao_voting_snip721_staked::msg::InstantiateMsg {
                unstaking_duration: None,
                nft_contract: dao_voting_snip721_staked::msg::NftContract::Existing {
                    address: nft_info.clone().address.to_string(),
                    code_hash: nft_info.clone().code_hash,
                },
                dao_code_hash,
                query_auth,
                active_threshold: None,
            })
            .unwrap(),
            admin: None,
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_info.code_id,
            code_hash: proposal_module_info.code_hash,
            msg: to_binary(&proposal_module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO governance module.".to_string(),
        }],
        initial_items: None,
        snip20_code_hash: "".to_string(),
        snip721_code_hash: "".to_string(),
    };

    let core_contract = app
        .instantiate_contract(
            core_contract_info,
            Addr::unchecked(CREATOR_ADDR),
            &instantiate_core,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    let core_state: dao_interface::query::DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract.clone().code_hash,
            core_contract.clone().address.to_string(),
            &dao_interface::msg::QueryMsg::DumpState {},
        )
        .unwrap();
    let staking_info = core_state.voting_module;

    for InitialBalance { address, amount } in initial_balances {
        for i in 0..amount.u128() {
            app.execute_contract(
                Addr::unchecked("ekez"),
                &nft_info.clone(),
                &snip721_reference_impl::msg::ExecuteMsg::MintNft {
                    token_id: Some(format!("{address}_{i}")),
                    owner: Some(address.clone()),
                    public_metadata: None,
                    private_metadata: None,
                    serial_number: None,
                    royalty_info: None,
                    transferable: Some(true),
                    memo: None,
                    padding: None,
                },
                &[],
            )
            .unwrap();
            app.execute_contract(
                Addr::unchecked(address.clone()),
                &nft_info.clone(),
                &snip721_reference_impl::msg::ExecuteMsg::SendNft {
                    contract: staking_info.clone().addr.to_string(),
                    token_id: format!("{address}_{i}"),
                    msg: Some(to_binary("").unwrap()),
                    receiver_info: Some(ReceiverInfo {
                        recipient_code_hash: staking_info.clone().code_hash,
                        also_implements_batch_receive_nft: Some(true),
                    }),
                    memo: None,
                    padding: None,
                },
                &[],
            )
            .unwrap();
        }
    }

    // Update the block so that staked balances appear.
    app.update_block(|block| block.height += 1);

    core_contract
}

pub(crate) fn _instantiate_with_native_staked_balances_governance(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_balances: Option<Vec<InitialBalance>>,
    query_auth: RawContract,
    dao_code_hash: String,
) -> ContractInfo {
    let proposal_module_instantiate_info = app.store_code(proposal_single_contract());

    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    // Collapse balances so that we can test double votes.
    let initial_balances: Vec<InitialBalance> = {
        let mut already_seen = vec![];
        initial_balances
            .into_iter()
            .filter(|InitialBalance { address, amount: _ }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .collect()
    };

    let native_stake_instantiate_info = app.store_code(native_staked_balances_voting_contract());
    let core_contract_instantiate_info = app.store_code(cw_core_contract());

    let instantiate_core = dao_interface::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        dao_uri: None,
        image_url: None,
        automatically_add_snip20s: true,
        automatically_add_snip721s: false,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: native_stake_instantiate_info.code_id,
            code_hash: native_stake_instantiate_info.code_hash,
            msg: to_binary(&dao_voting_token_staked::msg::InstantiateMsg {
                token_info: dao_voting_token_staked::msg::TokenInfo::Existing {
                    denom: "ujuno".to_string(),
                },
                unstaking_duration: None,
                active_threshold: None,
                query_auth,
                dao_code_hash,
            })
            .unwrap(),
            admin: None,
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_instantiate_info.code_id,
            code_hash: proposal_module_instantiate_info.code_hash,
            msg: to_binary(&proposal_module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO governance module.".to_string(),
        }],
        initial_items: None,
        snip20_code_hash: "".to_string(),
        snip721_code_hash: "".to_string(),
    };

    let core_contract = app
        .instantiate_contract(
            core_contract_instantiate_info.clone(),
            Addr::unchecked(CREATOR_ADDR),
            &instantiate_core,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    let gov_state: dao_interface::query::DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract.clone().code_hash,
            core_contract.clone().address.to_string(),
            &dao_interface::msg::QueryMsg::DumpState {},
        )
        .unwrap();
    let native_staking_contract_info = gov_state.voting_module;

    for InitialBalance { address, amount } in initial_balances {
        app.sudo(SudoMsg::Bank(BankSudo::Mint {
            to_address: address.clone(),
            amount: vec![Coin {
                denom: "ujuno".to_string(),
                amount,
            }],
        }))
        .unwrap();
        app.execute_contract(
            Addr::unchecked(&address),
            &ContractInfo {
                address: native_staking_contract_info.clone().addr,
                code_hash: native_staking_contract_info.clone().code_hash,
            },
            &dao_voting_token_staked::msg::ExecuteMsg::Stake {},
            &[Coin {
                amount,
                denom: "ujuno".to_string(),
            }],
        )
        .unwrap();
    }

    app.update_block(next_block);

    core_contract
}

pub(crate) fn _instantiate_with_staked_balances_governance(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_balances: Option<Vec<InitialBalance>>,
    dao_contract_instantiate_info: ContractInstantiationInfo,
    proposal_module_contract_info: ContractInstantiationInfo,
    query_auth: RawContract,
) -> ContractInfo {
    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    // Collapse balances so that we can test double votes.
    let initial_balances: Vec<InitialBalance> = {
        let mut already_seen = vec![];
        initial_balances
            .into_iter()
            .filter(|InitialBalance { address, amount: _ }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .collect()
    };

    let snip20_contract_info = instantiate_snip20(app, initial_balances.clone());
    let snip20_stake_contract_info = instantiate_staking(
        app,
        snip20_contract_info.clone().address,
        snip20_contract_info.clone().code_hash,
        Some(Duration::Height(6)),
        query_auth.clone(),
    );
    let staked_balances_voting_instantiate_info =
        app.store_code(snip20_staked_balances_voting_contract());

    let instantiate_core = dao_interface::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        dao_uri: None,
        image_url: None,
        automatically_add_snip20s: true,
        automatically_add_snip721s: false,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: staked_balances_voting_instantiate_info.code_id,
            code_hash: staked_balances_voting_instantiate_info.code_hash,
            msg: to_binary(&dao_voting_snip20_staked::msg::InstantiateMsg {
                active_threshold: None,
                token_info: dao_voting_snip20_staked::msg::Snip20TokenInfo::Existing {
                    address: snip20_contract_info.clone().address.to_string(),
                    code_hash: snip20_contract_info.clone().code_hash,
                    staking_contract: dao_voting_snip20_staked::msg::StakingInfo::Existing {
                        staking_contract_address: snip20_stake_contract_info
                            .clone()
                            .address
                            .to_string(),
                        staking_contract_code_hash: snip20_stake_contract_info.clone().code_hash,
                    },
                },
                dao_code_hash: dao_contract_instantiate_info.clone().code_hash,
                query_auth,
            })
            .unwrap(),
            admin: None,
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_contract_info.code_id,
            code_hash: proposal_module_contract_info.code_hash,
            msg: to_binary(&proposal_module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO governance module.".to_string(),
        }],
        initial_items: None,
        snip20_code_hash: "".to_string(),
        snip721_code_hash: "".to_string(),
    };

    let core_contract = app
        .instantiate_contract(
            dao_contract_instantiate_info,
            Addr::unchecked(CREATOR_ADDR),
            &instantiate_core,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    let gov_state: dao_interface::query::DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract.clone().code_hash,
            core_contract.clone().address.to_string(),
            &dao_interface::msg::QueryMsg::DumpState {},
        )
        .unwrap();
    let voting_module = gov_state.voting_module;

    let staking_contract: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_module.clone().code_hash,
            voting_module.clone().addr.to_string(),
            &dao_voting_snip20_staked::msg::QueryMsg::StakingContract {},
        )
        .unwrap();
    let token_contract: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_module.clone().code_hash,
            voting_module.clone().addr.to_string(),
            &dao_interface::voting::Query::TokenContract {},
        )
        .unwrap();

    // Stake all the initial balances.
    for InitialBalance { address, amount } in initial_balances {
        app.execute_contract(
            Addr::unchecked(address),
            &ContractInfo {
                address: token_contract.clone().addr,
                code_hash: token_contract.clone().code_hash,
            },
            &snip20_reference_impl::msg::ExecuteMsg::Send {
                recipient: staking_contract.clone().addr.to_string(),
                recipient_code_hash: Some(staking_contract.clone().code_hash),
                amount,
                msg: Some(to_binary(&snip20_stake::msg::ReceiveMsg::Stake {}).unwrap()),
                decoys: None,
                memo: None,
                entropy: None,
                padding: None,
            },
            &[],
        )
        .unwrap();
    }

    // Update the block so that those staked balances appear.
    app.update_block(|block| block.height += 1);

    core_contract
}

pub(crate) fn _instantiate_with_staking_active_threshold(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_balances: Option<Vec<InitialBalanceSnip20Staked>>,
    active_threshold: Option<ActiveThreshold>,
    dao_code_hash: String,
    query_auth: RawContract,
) -> ContractInfo {
    let proposal_module_contract_instantiate_info = app.store_code(proposal_single_contract());
    let snip20_instantiate_info = app.store_code(snip20_base_contract());
    let snip20_staking_instantiate_info = app.store_code(snip20_stake_contract());
    let core_instantiate_info = app.store_code(cw_core_contract());
    let voting_instantiate_info = app.store_code(snip20_staked_balances_voting_contract());

    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![InitialBalanceSnip20Staked {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    let governance_instantiate = dao_interface::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        dao_uri: None,
        image_url: None,
        automatically_add_snip20s: true,
        automatically_add_snip721s: true,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: voting_instantiate_info.code_id,
            code_hash: voting_instantiate_info.code_hash,
            msg: to_binary(&dao_voting_snip20_staked::msg::InstantiateMsg {
                token_info: dao_voting_snip20_staked::msg::Snip20TokenInfo::New {
                    code_id: snip20_instantiate_info.code_id,
                    code_hash: snip20_instantiate_info.code_hash,
                    name: "DAO".to_string(),
                    symbol: "DAO".to_string(),
                    decimals: 6,
                    initial_balances,
                    staking_code_id: snip20_staking_instantiate_info.code_id,
                    staking_code_hash: snip20_staking_instantiate_info.code_hash,
                    unstaking_duration: None,
                    initial_dao_balance: None,
                },
                dao_code_hash,
                query_auth,
                active_threshold,
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_contract_instantiate_info.code_id,
            code_hash: proposal_module_contract_instantiate_info.code_hash,
            msg: to_binary(&proposal_module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO governance module".to_string(),
        }],
        initial_items: None,
        snip20_code_hash: "".to_string(),
        snip721_code_hash: "".to_string(),
    };

    app.instantiate_contract(
        core_instantiate_info.clone(),
        Addr::unchecked(CREATOR_ADDR),
        &governance_instantiate,
        &[],
        "DAO DAO",
        None,
    )
    .unwrap()
}

pub(crate) fn _instantiate_with_cw4_groups_governance(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_weights: Option<Vec<InitialBalance>>,
    query_auth: RawContract,
    dao_code_hash: String,
) -> ContractInfo {
    let proposal_module_contract_instantiate_info = app.store_code(proposal_single_contract());
    let cw4_instantiate_info = app.store_code(cw4_group_contract());
    let core_instantiate_info = app.store_code(cw_core_contract());
    let votemod_instantiate_info = app.store_code(cw4_voting_contract());

    let initial_weights = initial_weights.unwrap_or_else(|| {
        vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(1),
        }]
    });

    // Remove duplicates so that we can test duplicate voting.
    let initial_weights: Vec<cw4::Member> = {
        let mut already_seen = vec![];
        initial_weights
            .into_iter()
            .filter(|InitialBalance { address, .. }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .map(|InitialBalance { address, amount }| cw4::Member {
                addr: address,
                weight: amount.u128() as u64,
            })
            .collect()
    };

    let governance_instantiate = dao_interface::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        dao_uri: None,
        image_url: None,
        automatically_add_snip20s: true,
        automatically_add_snip721s: true,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: votemod_instantiate_info.code_id,
            code_hash: votemod_instantiate_info.code_hash,
            msg: to_binary(&dao_voting_cw4::msg::InstantiateMsg {
                group_contract: GroupContract::New {
                    cw4_group_code_id: cw4_instantiate_info.code_id,
                    cw4_group_code_hash: cw4_instantiate_info.code_hash,
                    initial_members: initial_weights,
                },
                query_auth,
                dao_code_hash,
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_contract_instantiate_info.code_id,
            code_hash: proposal_module_contract_instantiate_info.code_hash,
            msg: to_binary(&proposal_module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO governance module".to_string(),
        }],
        initial_items: None,
        snip20_code_hash: "".to_string(),
        snip721_code_hash: "".to_string(),
    };

    let core_contract = app
        .instantiate_contract(
            core_instantiate_info.clone(),
            Addr::unchecked(CREATOR_ADDR),
            &governance_instantiate,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    // Update the block so that weights appear.
    app.update_block(|block| block.height += 1);

    core_contract
}

pub(crate) fn instantiate_query_auth(app: &mut App) -> ContractInfo {
    let query_auth_info = app.store_code(query_auth_contract());
    let msg = shade_protocol::contract_interfaces::query_auth::InstantiateMsg {
        admin_auth: shade_protocol::Contract {
            address: Addr::unchecked("admin_contract"),
            code_hash: "code_hash".to_string(),
        },
        prng_seed: to_binary("seed").unwrap(),
    };

    app.instantiate_contract(
        query_auth_info,
        Addr::unchecked(CREATOR_ADDR),
        &msg,
        &[],
        "query_auth",
        None,
    )
    .unwrap()
}

pub(crate) fn instantiate_staking(
    app: &mut App,
    snip20: Addr,
    snip20_code_hash: String,
    unstaking_duration: Option<Duration>,
    query_auth: RawContract,
) -> ContractInfo {
    let staking_info = app.store_code(snip20_stake_contract());
    let msg = snip20_stake::msg::InstantiateMsg {
        owner: Some(CREATOR_ADDR.to_string()),
        token_address: snip20.to_string(),
        unstaking_duration,
        token_code_hash: Some(snip20_code_hash),
        query_auth,
    };
    app.instantiate_contract(
        staking_info,
        Addr::unchecked(CREATOR_ADDR),
        &msg,
        &[],
        "staking",
        Some("admin".to_string()),
    )
    .unwrap()
}

pub(crate) fn instantiate_snip20(
    app: &mut App,
    initial_balances: Vec<InitialBalance>,
) -> ContractInfo {
    let snip20_info = app.store_code(snip20_base_contract());
    let msg = snip20_reference_impl::msg::InstantiateMsg {
        name: String::from("Test"),
        symbol: String::from("TEST"),
        decimals: 6,
        initial_balances: Some(initial_balances),
        admin: None,
        prng_seed: to_binary("seed").unwrap(),
        config: Some(snip20_reference_impl::msg::InitConfig {
            public_total_supply: Some(true),
            enable_deposit: None,
            enable_redeem: None,
            enable_mint: None,
            enable_burn: None,
            can_modify_denoms: None,
        }),
        supported_denoms: None,
    };

    app.instantiate_contract(
        snip20_info,
        Addr::unchecked(CREATOR_ADDR),
        &msg,
        &[],
        "snip20",
        None,
    )
    .unwrap()
}
