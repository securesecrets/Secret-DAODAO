use crate::testing::{
    contracts::{create_viewing_key, proposal_condorcet_contract},
    instantiation::instantiate_query_auth,
};
use cosmwasm_std::{coins, testing::mock_info, Addr, BankMsg, ContractInfo, CosmosMsg, Decimal};
use dao_interface::{state::AnyContractInfo, voting::InfoResponse};
use dao_voting::threshold::PercentageThreshold;
use secret_multi_test::{next_block, App, Executor};
use secret_utils::Duration;
use shade_protocol::basic_staking::Auth;

use crate::{
    config::{Config, UncheckedConfig},
    contract::{CONTRACT_NAME, CONTRACT_VERSION},
    msg::{Choice, ExecuteMsg, InstantiateMsg, QueryMsg},
    proposal::ProposalResponse,
};

pub(crate) struct Suite {
    app: App,
    sender: Addr,
    pub condorcet: Addr,
    pub condorcet_code_hash: String,
}

pub(crate) struct SuiteBuilder {
    pub instantiate: InstantiateMsg,
    with_proposal: Option<u32>,
    with_voters: Vec<(String, u64)>,
}

impl Default for SuiteBuilder {
    fn default() -> Self {
        Self {
            instantiate: UncheckedConfig {
                quorum: PercentageThreshold::Percent(Decimal::percent(15)),
                voting_period: Duration::Time(60 * 60 * 24 * 7),
                min_voting_period: Some(Duration::Time(60 * 60 * 24)),
                close_proposals_on_execution_failure: true,
                dao_code_hash: "dao_code_hash".to_string(),
            },
            with_proposal: None,
            with_voters: vec![("sender".to_string(), 10)],
        }
    }
}

impl SuiteBuilder {
    #[allow(clippy::field_reassign_with_default)]
    pub fn with_config(instantiate: UncheckedConfig) -> Self {
        let mut b = Self::default();
        b.instantiate = instantiate;
        b
    }

    pub fn _with_proposal(mut self, candidates: u32) -> Self {
        self.with_proposal = Some(candidates);
        self
    }

    pub fn _with_voters(mut self, voters: &[(&str, u64)]) -> Self {
        self.with_voters = voters.iter().map(|(a, p)| (a.to_string(), *p)).collect();
        self
    }

    pub fn build(self) -> Suite {
        let initial_members: Vec<_> = self
            .with_voters
            .into_iter()
            .map(|(addr, weight)| cw4::Member { addr, weight })
            .collect();
        let sender = Addr::unchecked(&initial_members[0].addr);

        let mut app = App::default();
        let condorcet_contract_instantiation_info = app.store_code(proposal_condorcet_contract());
        // let core_contract_instantiation_info = app.store_code(dao_dao_contract());
        // let cw4_contract_instantiation_info = app.store_code(cw4_group_contract());
        // let cw4_voting_contract_instantiation_info = app.store_code(dao_voting_cw4_contract());
        let query_auth = instantiate_query_auth(&mut app);

        let proposal_condorcet_info = app
            .instantiate_contract(
                condorcet_contract_instantiation_info,
                sender.clone(),
                &self.instantiate,
                &[],
                "proposal_condorcet".to_string(),
                None,
            )
            .unwrap();

        app.update_block(next_block);

        let mut suite = Suite {
            app,
            sender,
            condorcet: proposal_condorcet_info.address,
            condorcet_code_hash: proposal_condorcet_info.code_hash,
        };

        let next_id = suite.query_next_proposal_id();
        assert_eq!(next_id, 1);

        let viewing_key_sender = create_viewing_key(
            &mut suite.app,
            query_auth,
            mock_info(suite.sender.as_ref(), &[]),
        );

        if let Some(candidates) = self.with_proposal {
            suite
                .propose(
                    &suite.sender(),
                    Auth::ViewingKey {
                        key: viewing_key_sender,
                        address: suite.sender.to_string(),
                    },
                    (0..candidates)
                        .map(|_| vec![unimportant_message()])
                        .collect(),
                )
                .unwrap();
            let next_id = suite.query_next_proposal_id();
            assert_eq!(next_id, 2);
        }

        let info = suite.query_info();
        assert_eq!(info.info.version, CONTRACT_VERSION);
        assert_eq!(info.info.contract, CONTRACT_NAME);

        suite
    }
}

impl Suite {
    pub fn _block_height(&self) -> u64 {
        self.app.block_info().height
    }

    // pub fn a_day_passes(&mut self) {
    //     self.app
    //         .update_block(|b| b.time = b.time.plus_seconds(60 * 60 * 24))
    // }

    // pub fn a_week_passes(&mut self) {
    //     self.a_day_passes();
    //     self.a_day_passes();
    //     self.a_day_passes();
    //     self.a_day_passes();
    //     self.a_day_passes();
    //     self.a_day_passes();
    //     self.a_day_passes();
    // }

    pub fn sender(&self) -> Addr {
        self.sender.clone()
    }
}

// query
impl Suite {
    pub fn query_config(&self) -> Config {
        self.app
            .wrap()
            .query_wasm_smart(
                &self.condorcet_code_hash,
                &self.condorcet,
                &QueryMsg::Config {},
            )
            .unwrap()
    }

    pub fn _query_proposal(&self, id: u32) -> ProposalResponse {
        self.app
            .wrap()
            .query_wasm_smart(
                &self.condorcet_code_hash,
                &self.condorcet,
                &QueryMsg::Proposal { id },
            )
            .unwrap()
    }

    // pub fn query_winner_and_status(&self, id: u32) -> (Winner, Status) {
    //     let q = self.query_proposal(id);
    //     (q.tally.winner, q.proposal.last_status())
    // }

    pub fn query_next_proposal_id(&self) -> u32 {
        self.app
            .wrap()
            .query_wasm_smart(
                &self.condorcet_code_hash,
                &self.condorcet,
                &QueryMsg::NextProposalId {},
            )
            .unwrap()
    }

    pub fn _query_dao(&self) -> AnyContractInfo {
        self.app
            .wrap()
            .query_wasm_smart(
                &self.condorcet_code_hash,
                &self.condorcet,
                &QueryMsg::Dao {},
            )
            .unwrap()
    }

    pub fn query_info(&self) -> InfoResponse {
        self.app
            .wrap()
            .query_wasm_smart(
                &self.condorcet_code_hash,
                &self.condorcet,
                &QueryMsg::Info {},
            )
            .unwrap()
    }
}

// execute
impl Suite {
    pub fn propose<S: Into<String>>(
        &mut self,
        sender: S,
        auth: Auth,
        choices: Vec<Vec<CosmosMsg>>,
    ) -> anyhow::Result<u32> {
        let id = self.query_next_proposal_id();
        self.app.execute_contract(
            Addr::unchecked(sender),
            &ContractInfo {
                address: self.condorcet.clone(),
                code_hash: self.condorcet_code_hash.clone(),
            },
            &ExecuteMsg::Propose {
                auth,
                choices: choices.into_iter().map(|msgs| Choice { msgs }).collect(),
            },
            &[],
        )?;
        Ok(id)
    }

    pub fn _vote<S: Into<String>>(
        &mut self,
        sender: S,
        auth: Auth,
        proposal_id: u32,
        vote: Vec<u32>,
    ) -> anyhow::Result<()> {
        self.app
            .execute_contract(
                Addr::unchecked(sender),
                &ContractInfo {
                    address: self.condorcet.clone(),
                    code_hash: self.condorcet_code_hash.clone(),
                },
                &ExecuteMsg::Vote {
                    auth,
                    proposal_id,
                    vote,
                },
                &[],
            )
            .map(|_| ())
    }

    pub fn _execute<S: Into<String>>(
        &mut self,
        sender: S,
        auth: Auth,
        proposal_id: u32,
    ) -> anyhow::Result<()> {
        self.app
            .execute_contract(
                Addr::unchecked(sender),
                &ContractInfo {
                    address: self.condorcet.clone(),
                    code_hash: self.condorcet_code_hash.clone(),
                },
                &ExecuteMsg::Execute { auth, proposal_id },
                &[],
            )
            .map(|_| ())
    }

    pub fn _close<S: Into<String>>(&mut self, sender: S, proposal_id: u32) -> anyhow::Result<()> {
        self.app
            .execute_contract(
                Addr::unchecked(sender),
                &ContractInfo {
                    address: self.condorcet.clone(),
                    code_hash: self.condorcet_code_hash.clone(),
                },
                &ExecuteMsg::Close { proposal_id },
                &[],
            )
            .map(|_| ())
    }
}

pub fn unimportant_message() -> CosmosMsg {
    BankMsg::Send {
        to_address: "someone".to_string(),
        amount: coins(10, "something"),
    }
    .into()
}
