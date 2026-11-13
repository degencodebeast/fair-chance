//! Fair Chance factory. `launch` creates one coin account in one batch: CreateAccount,
//! Transfer, UseGlobalContract by code hash, FunctionCall `new`. It never adds a key.

use near_sdk::json_types::{Base58CryptoHash, U128};
use near_sdk::serde_json::json;
use near_sdk::{env, near, AccountId, Gas, NearToken, PanicOnDefault, Promise};

const NEW_GAS: Gas = Gas::from_tgas(50);
const CALLBACK_GAS: Gas = Gas::from_tgas(10);

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

    /// Launch a coin at `<ticker>.<factory>`. The attached NEAR pays the coin's storage.
    #[payable]
    pub fn launch(
        &mut self,
        name: String,
        ticker: String,
        icon: Option<String>,
        description: Option<String>,
        opening_buy: Option<U128>,
    ) -> Promise {
        let _ = description;
        if opening_buy.is_some() {
            env::panic_str("opening_buy is not in this build");
        }
        let creator = env::predecessor_account_id();
        let coin: AccountId = format!("{}.{}", ticker.to_lowercase(), env::current_account_id())
            .parse()
            .unwrap_or_else(|_| env::panic_str("ticker does not make a valid account ID"));
        let code_hash = String::from(&self.coin_code_hash);
        let args = json!({
            "creator": creator,
            "name": name,
            "symbol": ticker,
            "icon": icon,
            "code_hash": code_hash,
        });
        let hash: [u8; 32] = self.coin_code_hash.into();
        Promise::new(coin.clone())
            .create_account()
            .transfer(env::attached_deposit())
            .use_global_contract(hash)
            .function_call(
                "new",
                args.to_string().into_bytes(),
                NearToken::from_yoctonear(0),
                NEW_GAS,
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .on_launch(coin, creator, env::attached_deposit()),
            )
    }

    /// Emits `fc_launch` only when the batch succeeded; else returns the deposit.
    #[private]
    pub fn on_launch(&mut self, coin: AccountId, creator: AccountId, deposit: NearToken) -> bool {
        if env::promise_result_checked(0, 0).is_ok() {
            let event = json!({
                "standard": "fair_chance_factory",
                "version": rule_engine::INTERFACE_VERSION,
                "event": "fc_launch",
                "data": [{
                    "coin": coin,
                    "creator": creator,
                    "code_hash": String::from(&self.coin_code_hash),
                    "launch_block": env::block_height(),
                }],
            });
            env::log_str(&format!("EVENT_JSON:{event}"));
            true
        } else {
            Promise::new(creator).transfer(deposit).detach();
            false
        }
    }

    pub fn coin_code_hash(&self) -> Base58CryptoHash {
        self.coin_code_hash
    }
}
