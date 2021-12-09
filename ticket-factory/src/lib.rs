use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, ext_contract, near_bindgen, AccountId, Balance, Gas, PanicOnDefault, Promise, log, PromiseResult};

near_sdk::setup_alloc!();
const CODE: &[u8] = include_bytes!("../../ticket/res/contract.wasm");
const INITIAL_BALANCE: Balance = 6_500_000_000_000_000_000_000_000;
const CREATE_CONTRACT_FEE: Balance = 5_000_000_000_000_000_000_000_000;
const PREPARE_GAS: Gas = 25_000_000_000_000;
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    pub owner_id: AccountId,
    pub ticket_contracts_by_owner: UnorderedMap<AccountId, Vec<AccountId>>,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        Self {
            owner_id,
            ticket_contracts_by_owner: UnorderedMap::new(b"ticket_contract_by_owner".to_vec()),
        }
    }
    #[payable]
    pub fn create_new_ticket_contract(&mut self, prefix: String, metadata: TicketContractMetadata) -> Promise {
        assert!(
            env::attached_deposit() == CREATE_CONTRACT_FEE + INITIAL_BALANCE,
            "Not enough Near to create contract"
        );
        let subaccount_id = format!("{}.{}", prefix, env::current_account_id());
        log!("{}", format!("Creating new ticket contract at account {}", subaccount_id));
        let mut ticket_contracts = self.ticket_contracts_by_owner.get(&env::predecessor_account_id()).unwrap_or_else(|| Vec::new());
        ticket_contracts.push(subaccount_id.clone());
        self.ticket_contracts_by_owner.insert(&env::predecessor_account_id(), &ticket_contracts);
        Promise::new(subaccount_id.clone())
            .create_account()
            .transfer(INITIAL_BALANCE)
            .add_full_access_key(env::signer_account_pk())
            .deploy_contract(CODE.to_vec())
            .then(new_ticket_contract::new(
                env::predecessor_account_id(),
                metadata,
                &subaccount_id,
                0,
                PREPARE_GAS,
            ))
            .then(ex_self::check_create_new_contract(
                env::predecessor_account_id(),
                &env::current_account_id(),
                0,
                5_000_000_000_000
            ))
    }
    #[private]
    pub fn check_create_new_contract(&mut self, creater_account: AccountId) {
        let mut result: bool = true;
        for i in 0..env::promise_results_count(){
            if env::promise_result(i) == PromiseResult::Failed {
                result = false; 
                break
            }
        };
        if result == false {
            log!("Fail to create new ticket contract");
            Promise::new(creater_account).transfer(INITIAL_BALANCE + CREATE_CONTRACT_FEE);
        }
    }
    pub fn get_contracts_by_owner(&self, owner_id: AccountId) -> Vec<AccountId>{
        self.ticket_contracts_by_owner.get(&owner_id).unwrap_or_else(|| Vec::new())
    }
}

#[ext_contract(new_ticket_contract)]
trait TTicketContract {
    fn new(owner_id: AccountId, metadata: TicketContractMetadata) -> Self;
}
#[ext_contract(ex_self)]
trait TContractSelf{
    fn check_create_new_contract(&mut self, creater_account: AccountId);
}


#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct TicketContractMetadata {
    pub spec: String,   // required, essentially a version like "nft-1.0.0"
    pub name: String,   // required, ex. "Mosaics"
    pub symbol: String, // required, ex. "MOSIAC"
    pub description: Option<String>,
}
