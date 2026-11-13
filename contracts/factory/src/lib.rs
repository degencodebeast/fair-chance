//! Fair Chance factory. `launch` creates one coin account in one batch: CreateAccount,
//! Transfer, UseGlobalContract by code hash, FunctionCall `new`. It never adds a key.

use near_sdk::json_types::{Base58CryptoHash, U128};
use near_sdk::serde_json::{json, Value};
use near_sdk::{env, near, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseOrValue};

const NEW_GAS: Gas = Gas::from_tgas(50);
const ON_LAUNCH_GAS: Gas = Gas::from_tgas(40);
const RULES_READ_GAS: Gas = Gas::from_tgas(10);
const ON_COIN_RULES_GAS: Gas = Gas::from_tgas(10);
/// Upper bound on the `fc_rules` answer the factory reads back from a new coin.
const RULES_MAX_BYTES: usize = 4096;

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Factory {
    coin_code_hash: Base58CryptoHash,
}

#[near]
impl Factory {
    #[init]
    pub fn new(coin_code_hash: Base58CryptoHash) -> Self {
        Self { coin_code_hash }
    }

    /// Launch a coin at `<ticker>.<factory>`. The attached NEAR pays the coin's storage, and
    /// `opening_buy` (yoctoNEAR, part of the attached NEAR) is the creator's buy inside `new`.
    #[payable]
    pub fn launch(
        &mut self,
        name: String,
        ticker: String,
        icon: Option<String>,
        description: Option<String>,
        opening_buy: Option<U128>,
    ) -> Promise {
        let creator = env::predecessor_account_id();
        let deposit = env::attached_deposit();
        let opening_buy = NearToken::from_yoctonear(opening_buy.map_or(0, |b| b.0));
        if opening_buy >= deposit && !opening_buy.is_zero() {
            env::panic_str("opening_buy must be less than the attached deposit, which also pays the coin's storage");
        }
        let coin: AccountId = format!("{}.{}", ticker.to_lowercase(), env::current_account_id())
            .parse()
            .unwrap_or_else(|_| env::panic_str("ticker does not make a valid account ID"));
        let args = json!({
            "creator": creator,
            "name": name,
            "symbol": ticker,
            "icon": icon,
            "description": description,
            "code_hash": String::from(&self.coin_code_hash),
        });
        let hash: [u8; 32] = self.coin_code_hash.into();
        Promise::new(coin.clone())
            .create_account()
            .transfer(deposit.saturating_sub(opening_buy))
            .use_global_contract(hash)
            .function_call("new", args.to_string().into_bytes(), opening_buy, NEW_GAS)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(ON_LAUNCH_GAS)
                    .on_launch(coin, creator, deposit),
            )
    }

    /// After the batch: on failure, return the deposit (the batch rolled back, so no coin
    /// account exists). On success, read the coin's `fc_rules` to log `fc_launch`.
    #[private]
    pub fn on_launch(
        &mut self,
        coin: AccountId,
        creator: AccountId,
        deposit: NearToken,
    ) -> PromiseOrValue<bool> {
        if env::promise_result_checked(0, 0).is_err() {
            Promise::new(creator).transfer(deposit).detach();
            return PromiseOrValue::Value(false);
        }
        Promise::new(coin.clone())
            .function_call(
                "fc_rules",
                b"{}".to_vec(),
                NearToken::from_yoctonear(0),
                RULES_READ_GAS,
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(ON_COIN_RULES_GAS)
                    .on_coin_rules(coin, creator),
            )
            .into()
    }

    /// Logs `fc_launch` with the block and description the coin recorded at `new`.
    #[private]
    pub fn on_coin_rules(&mut self, coin: AccountId, creator: AccountId) -> bool {
        let rules: Value = env::promise_result_checked(0, RULES_MAX_BYTES)
            .ok()
            .and_then(|bytes| near_sdk::serde_json::from_slice(&bytes).ok())
            .unwrap_or_else(|| {
                env::panic_str("the coin exists but its fc_rules could not be read")
            });
        let event = json!({
            "standard": "fair_chance_factory",
            "version": rule_engine::INTERFACE_VERSION,
            "event": "fc_launch",
            "data": [{
                "coin": coin,
                "creator": creator,
                "code_hash": String::from(&self.coin_code_hash),
                "launch_block": rules["launch_block"],
                "description": rules["description"],
            }],
        });
        env::log_str(&format!("EVENT_JSON:{event}"));
        true
    }

    pub fn coin_code_hash(&self) -> Base58CryptoHash {
        self.coin_code_hash
    }
}
