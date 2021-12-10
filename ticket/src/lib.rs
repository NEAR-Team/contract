/*!
Non-Fungible Token implementation with JSON serialization.
NOTES:
  - The maximum balance value is limited by U128 (2**128 - 1).
  - JSON calls should pass U128 as a base-10 string. E.g. "100".
  - The contract optimizes the inner trie structure by hashing account IDs. It will prevent some
    abuse of deep tries. Shouldn't be an issue, once NEAR clients implement full hashing of keys.
  - The contract tracks the change in storage before and after the call. If the storage increases,
    the contract requires the caller of the contract to attach enough deposit to the function call
    to cover the storage cost.
    This is done to prevent a denial of service attack on the contract by taking all available storage.
    If the storage decreases, the contract will issue a refund for the cost of the released storage.
    The unused tokens from the attached deposit are also refunded, so it's safe to
    attach more deposit than required.
  - To prevent the deployed contract from being modified or deleted, it should not have any access
    keys on its account.
*/
use near_contract_standards::non_fungible_token::metadata::TokenMetadata;
use near_contract_standards::non_fungible_token::NonFungibleToken;
use near_contract_standards::non_fungible_token::{Token, TokenId};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedMap, UnorderedSet};
use near_sdk::json_types::ValidAccountId;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    assert_one_yocto, env, ext_contract, log, near_bindgen, AccountId, Balance, BorshStorageKey,
    Gas, PanicOnDefault, Promise, PromiseOrValue, Timestamp,
};

const MINT_FEE: Balance = 1_000_000_000_000_000_000_000_0;
const PREPARE_GAS: Gas = 1_500_000_000_000_0;
near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    owner_id: AccountId,
    tokens: NonFungibleToken,
    metadata: LazyOption<TicketContractMetadata>,
    shows: UnorderedMap<String, ShowMetadata>,
    tickets: UnorderedMap<TokenId, TicketMetadata>,
}

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    NonFungibleToken,
    Metadata,
    TokenMetadata,
    Enumeration,
    Approval,
    ShowMetadata,
    TicketMetadata,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(owner_id: AccountId, metadata: TicketContractMetadata) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self {
            owner_id,
            tokens: NonFungibleToken::new(
                StorageKey::NonFungibleToken,
                ValidAccountId::try_from(env::current_account_id()).unwrap(),
                Some(StorageKey::TokenMetadata),
                Some(StorageKey::Enumeration),
                Some(StorageKey::Approval),
            ),
            metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
            shows: UnorderedMap::new(StorageKey::ShowMetadata),
            tickets: UnorderedMap::new(StorageKey::TicketMetadata),
        }
    }

    pub fn transfer_ownership(&mut self, new_owner: ValidAccountId) {
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "Caller is not owner"
        );
        self.owner_id = new_owner.into();
    }

    pub fn renounce_ownership(&mut self) {
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "Caller is not owner"
        );
        self.owner_id = String::new();
    }

    pub fn create_new_show(
        &mut self,
        show_id: String, // required,
        show_title: Option<String>,
        show_description: Option<String>,
        ticket_types: Vec<String>,     // required, type ticket => amount
        tickets_supply: Vec<u64>,      // required
        ticket_prices: Vec<f64>,   // required, type ticket =>
        selling_start_time: Timestamp, // required
        selling_end_time: Timestamp,
    ) {
        assert!(self.shows.get(&show_id).is_none(), "This show exist");
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "Caller is not owner"
        );
        let mut ticket_sold_by_type = HashMap::new();
        let mut total_supply_ticket_by_type = HashMap::new();
        let mut ticket_price_by_type = HashMap::new();
        for i in 0..ticket_types.len() {
            total_supply_ticket_by_type.insert(ticket_types[i].clone(), tickets_supply[i]);
            ticket_sold_by_type.insert(ticket_types[i].clone(), 0u64);
            let price: Balance = (ticket_prices[i] * 1_000_000_000_000_000_000_000_000u128 as f64).round() as Balance + MINT_FEE;
            ticket_price_by_type.insert(ticket_types[i].clone(), price);
        }
        self.shows.insert(
            &show_id.clone(),
            &ShowMetadata {
                show_id,
                show_title,
                show_description,
                ticket_sold_by_type,
                total_supply_ticket_by_type,
                ticket_price_by_type,
                selling_start_time,
                selling_end_time,
            },
        );
    }
    #[payable]
    pub fn buy_ticket(&mut self, show_id: String, ticket_type: String) -> Promise {
        let show = self.shows.get(&show_id).unwrap();
        assert!(
            env::block_timestamp() > show.selling_start_time,
            "This show has not started selling tickets yet"
        );
        assert!(
            env::block_timestamp() < show.selling_end_time,
            "This show has ended ticket sales"
        );
        assert!(
            show.ticket_sold_by_type.get(&ticket_type).unwrap()
                < show.total_supply_ticket_by_type.get(&ticket_type).unwrap(),
            "All tickets are sold out"
        );
        assert!(
            env::attached_deposit()
                == *show.ticket_price_by_type.get(&ticket_type).unwrap() + MINT_FEE,
            "Please deposit exactly price of ticket"
        );
        let ticket_id = format!(
            "{}.{}.{}",
            show_id,
            ticket_type,
            show.ticket_sold_by_type.get(&ticket_type).unwrap()
        );
        log!(
            "{}",
            format!(
                "Buy new ticket: show id: {}, ticket type: {}, ticket id: {}, price: {} YoctoNear",
                show_id,
                ticket_type,
                ticket_id,
                show.ticket_price_by_type.get(&ticket_type).unwrap()
            )
        );
        ex_self::nft_private_mint(
            ticket_id,
            ValidAccountId::try_from(env::predecessor_account_id()).unwrap(),
            &env::current_account_id(),
            MINT_FEE,
            PREPARE_GAS,
        )
    }
    // pub fn buy_tickets(&mut self, show_id: String, ticket_type: String, amount: u64) {
    //     let show = self.shows.get(&show_id).unwrap();
    //     assert!(
    //         env::block_timestamp() > show.selling_start_time,
    //         "This show has not started selling tickets yet"
    //     );
    //     assert!(
    //         env::block_timestamp() < show.selling_end_time,
    //         "This show has ended ticket sales"
    //     );
    //     assert!(
    //         *show.ticket_sold_by_type.get(&ticket_type).unwrap() + amount
    //             < *show.total_supply_ticket_by_type.get(&ticket_type).unwrap(),
    //         "All tickets are sold out"
    //     );
    //     assert!(
    //         env::attached_deposit()
    //             == *show.ticket_price_by_type.get(&ticket_type).unwrap() + MINT_FEE,
    //         "Please deposit exactly price of ticket"
    //     );
    // }

    #[payable]
    pub fn check_ticket(&mut self, ticket_id: String) {
        assert_one_yocto();
        assert!(
            self.tokens.owner_by_id.get(&ticket_id) == Some(env::predecessor_account_id()),
            "You do not own the ticket"
        );
        let mut ticket = self
            .tickets
            .get(&ticket_id)
            .unwrap_or_else(|| env::panic(b"ticket id does not exist!"));
        ticket.is_used = true;
        self.tickets.insert(&ticket_id, &ticket);
        log!("{}", format!("Ticket {} is checked", ticket_id));
    }
    #[payable]
    #[private]
    pub fn nft_private_mint(&mut self, token_id: TokenId, receiver_id: ValidAccountId) -> Token {
        let token_id_split: Vec<&str> = token_id.split(".").collect();
        let show_id = token_id_split[0].to_string();
        let ticket_type = token_id_split[1].to_string();
        let mut count: u64 = token_id_split[2].parse().unwrap();
        count += 1;
        let mut show = self.shows.get(&show_id).unwrap();
        show.ticket_sold_by_type.insert(ticket_type.clone(), count);
        self.shows.insert(&show_id, &show);
        self.tickets.insert(
            &token_id,
            &TicketMetadata {
                ticket_id: token_id.clone(),
                show_id,
                ticket_type,
                is_used: false,
                issued_at: env::block_timestamp(),
            },
        );
        self.tokens.mint(
            token_id,
            receiver_id,
            Some(TokenMetadata {
                title: None,       // ex. "Arch Nemesis: Mail Carrier" or "Parcel #5055"
                description: None, // free-form description
                media: None, // URL to associated media, preferably to decentralized, content-addressed storage
                media_hash: None, // Base64-encoded sha256 hash of content referenced by the `media` field. Required if `media` is included.
                copies: None, // number of copies of this set of metadata in existence when token was minted.
                issued_at: Some(env::block_timestamp().to_string()), // ISO 8601 datetime when token was issued or minted
                expires_at: None,     // ISO 8601 datetime when token expires
                starts_at: None,      // ISO 8601 datetime when token starts being valid
                updated_at: None,     // ISO 8601 datetime when token was last updated
                extra: None, // anything extra the NFT wants to store on-chain. Can be stringified JSON.
                reference: None, // URL to an off-chain JSON file with more info.
                reference_hash: None, // Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
            }),
        )
    }

    pub fn show_metadata(&self, show_id: String) -> ShowMetadata {
        self.shows.get(&show_id).unwrap()
    }

    pub fn ticket_metadata(&self, token_id: TokenId) -> TicketMetadata {
        self.tickets.get(&token_id).unwrap()
    }

    pub fn get_tickets_by_owner(&self, owner: AccountId) -> Vec<TicketMetadata> {
        let token_ids = self
            .tokens
            .tokens_per_owner
            .as_ref()
            .unwrap()
            .get(&owner)
            .unwrap_or_else(|| UnorderedSet::new(b"".to_vec()));
        token_ids
            .iter()
            .map(|token_id| self.tickets.get(&token_id).unwrap())
            .collect()
    }
}

near_contract_standards::impl_non_fungible_token_core!(Contract, tokens);
near_contract_standards::impl_non_fungible_token_approval!(Contract, tokens);
near_contract_standards::impl_non_fungible_token_enumeration!(Contract, tokens);

#[near_bindgen]
impl Contract {
    pub fn ticket_contract_metadata(&self) -> TicketContractMetadata {
        self.metadata.get().unwrap()
    }
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct TicketContractMetadata {
    pub spec: String,   // required, essentially a version like "nft-1.0.0"
    pub name: String,   // required, ex. "Mosaics"
    pub symbol: String, // required, ex. "MOSIAC"
    pub description: Option<String>,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct TicketMetadata {
    pub ticket_id: String,   // required
    pub show_id: String,     // required,
    pub ticket_type: String, // required,
    pub is_used: bool,       // required,
    issued_at: Timestamp,    // required
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct ShowMetadata {
    pub show_id: String, // required,
    pub show_title: Option<String>,
    pub show_description: Option<String>,
    pub total_supply_ticket_by_type: HashMap<String, u64>, // required, type ticket => amount
    pub ticket_sold_by_type: HashMap<String, u64>,         // required, type ticket => sold amount
    pub ticket_price_by_type: HashMap<String, Balance>,    // required, type ticket =>
    pub selling_start_time: Timestamp,                     // required
    pub selling_end_time: Timestamp,                       // required
}

#[ext_contract(ex_self)]
trait TTicketContract {
    fn nft_private_mint(&mut self, token_id: TokenId, receiver_id: ValidAccountId) -> Token;
}
