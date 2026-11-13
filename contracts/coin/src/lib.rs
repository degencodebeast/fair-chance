//! Fair Chance coin contract. Thin wrappers: every rule is in `fc_rules`.

use near_sdk::json_types::{U128, U64};
use near_sdk::store::LookupMap;
use near_sdk::{
    assert_one_yocto, env, log, near, AccountId, Gas, NearToken, PanicOnDefault, Promise,
    PromiseOrValue,
};
use rule_engine::{
    AccountView, Action, Applied, At, BuyQuote, CapView, CoinState, CreatorFeesView,
    EarlyWindowView, FixedValues, Holder, Holders, LaunchRequest, Payout, PoolView, Refusal,
    RefusalCode, RulesView, SellQuote, Unregistered,
};

/// Storage key prefix of the holder records.
const HOLDERS_PREFIX: &[u8] = b"h";
/// Bytes the chain counts for each stored record on top of its key and value.
const RECORD_OVERHEAD_BYTES: u64 = 40;
/// The longest NEAR account ID.
const LONGEST_ACCOUNT_ID_BYTES: u64 = 64;
/// Gas for `on_creator_claim`, the claim payout callback.
const ON_CREATOR_CLAIM_GAS: Gas = Gas::from_tgas(10);

#[derive(Clone)]
#[near(serializers = [json])]
pub struct FungibleTokenMetadata {
    pub spec: String,
    pub name: String,
    pub symbol: String,
    pub icon: Option<String>,
    pub reference: Option<String>,
    pub reference_hash: Option<String>,
    pub decimals: u8,
}

/// NEP-145 storage balance of one account, in yoctoNEAR.
#[near(serializers = [json])]
pub struct StorageBalance {
    pub total: NearToken,
    pub available: NearToken,
}

/// NEP-145 storage bounds: one record costs `min`, and `max` equals `min`.
#[near(serializers = [json])]
pub struct StorageBalanceBounds {
    pub min: NearToken,
    pub max: Option<NearToken>,
}

#[near(
    contract_state,
    contract_metadata(
        standard(standard = "nep141", version = "1.0.0"),
        standard(standard = "nep148", version = "1.0.0"),
        standard(standard = "fair_chance", version = "1.0.0-rc.6")
    )
)]
#[derive(PanicOnDefault)]
pub struct Coin {
    state: CoinState,
    holders: LookupMap<String, Holder>,
}

fn now() -> At {
    At {
        block: env::block_height(),
        ns: env::block_timestamp(),
    }
}

#[near]
impl Coin {
    /// The one-time start of the coin. Any attached NEAR is the creator's opening buy, made at
    /// the launch block under the same rules as every buy.
    #[init]
    #[payable]
    pub fn new(
        creator: AccountId,
        name: String,
        symbol: String,
        icon: Option<String>,
        description: Option<String>,
        code_hash: String,
    ) -> Self {
        let coin = env::current_account_id();
        let caller = env::predecessor_account_id();
        let launched = rule_engine::launch(
            FixedValues::fixed(),
            LaunchRequest {
                coin: coin.as_str(),
                caller: caller.as_str(),
                creator: creator.as_str(),
                code_hash: &code_hash,
                name: &name,
                symbol: &symbol,
                icon: icon.as_deref(),
                description: description.as_deref(),
            },
            now(),
        )
        .unwrap_or_else(|r| env::panic_str(&r.to_string()));
        for event in &launched.events {
            env::log_str(&event.log_line());
        }
        let mut started = Self {
            state: launched.state,
            holders: LookupMap::new(HOLDERS_PREFIX),
        };
        let opening_buy = env::attached_deposit().as_yoctonear();
        if opening_buy > 0 {
            started.commit(&Action::Buy {
                account: creator.to_string(),
                near_in: opening_buy,
                min_tokens_out: 0,
            });
        }
        // Write the state now so storage_usage counts all of it. The pool's real NEAR must stay
        // payable, so the storage lock must come from the rest of the balance.
        started.holders.flush();
        env::state_write(&started);
        let storage_lock =
            env::storage_byte_cost().as_yoctonear() * u128::from(env::storage_usage());
        let free = env::account_balance()
            .as_yoctonear()
            .saturating_sub(started.state.real_near);
        if free < storage_lock {
            env::panic_str("the launch deposit does not cover the coin's storage");
        }
        started
    }

    /// Buy tokens with the attached NEAR. Refused with `FC_SLIPPAGE` below `min_tokens_out`.
    /// The buyer needs a NEP-145 storage record first, so the pool's NEAR never pays for one.
    /// (The creator's opening buy in `new` is paid by the launch deposit.)
    #[payable]
    pub fn buy(&mut self, min_tokens_out: U128) {
        let buyer = env::predecessor_account_id();
        if !self.holders.contains_key(buyer.as_str()) {
            env::panic_str(
                &Refusal::with_detail(RefusalCode::NotRegistered, buyer.as_str()).to_string(),
            );
        }
        let action = Action::Buy {
            account: buyer.to_string(),
            near_in: env::attached_deposit().as_yoctonear(),
            min_tokens_out: min_tokens_out.0,
        };
        self.commit(&action);
    }

    /// Sell `amount` tokens. Needs exactly 1 yoctoNEAR. `min_near_out` is the NEAR accepted for
    /// all of `amount`; the check is on the price per token of the part that fills.
    #[payable]
    pub fn sell(&mut self, amount: U128, min_near_out: U128) -> Option<Promise> {
        let action = Action::Sell {
            account: env::predecessor_account_id().to_string(),
            amount: amount.0,
            min_near_out: min_near_out.0,
            attached: env::attached_deposit().as_yoctonear(),
        };
        self.commit(&action)
    }

    /// The creator claims its whole fee balance (ADR 0010). Needs exactly 1 yoctoNEAR. Refused
    /// with `FC_NOT_CREATOR` for any other account and with `FC_CLAIM_NOT_READY` before 24 h of
    /// pool depth or with a zero balance. The balance is 0 from now on; `on_creator_claim` settles
    /// the transfer.
    #[payable]
    pub fn claim_creator_fees(&mut self) -> Promise {
        let creator = env::predecessor_account_id();
        let action = Action::Claim {
            account: creator.to_string(),
            attached: env::attached_deposit().as_yoctonear(),
        };
        let payout = self
            .run(&action)
            .unwrap_or_else(|| env::panic_str("the claim has no payout"));
        let depth_ok_since_ns = rule_engine::view_creator_fees(&self.state, now())
            .depth_ok_since_ns
            .unwrap_or_else(|| env::panic_str("the claim has no depth_ok_since"));
        Promise::new(creator)
            .transfer(NearToken::from_yoctonear(payout.amount))
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(ON_CREATOR_CLAIM_GAS)
                    .on_creator_claim(U128(payout.amount), U64(depth_ok_since_ns)),
            )
    }

    /// The claim payout callback. On success it logs `fc_creator_claim`; on failure (for example,
    /// the creator account no longer exists) it puts the amount back in the creator balance.
    /// Returns true when the transfer succeeded.
    #[private]
    pub fn on_creator_claim(&mut self, amount: U128, depth_ok_since_ns: U64) -> bool {
        let succeeded = env::promise_result_checked(0, 0).is_ok();
        let settled = rule_engine::settle_creator_claim(
            &self.state,
            amount.0,
            depth_ok_since_ns.0,
            succeeded,
            now(),
        );
        self.state = settled.state;
        for event in &settled.events {
            env::log_str(&event.log_line());
        }
        succeeded
    }

    /// NEP-141 send. Needs exactly 1 yoctoNEAR and a receiver with a storage record.
    #[payable]
    pub fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        assert_one_yocto();
        let action = Action::Send {
            from: env::predecessor_account_id().to_string(),
            to: receiver_id.to_string(),
            amount: amount.0,
            memo,
        };
        self.commit(&action);
    }

    /// Refused for every receiver with `FC_TRANSFER_CALL` (ADR 0001: no outside liquidity).
    #[payable]
    pub fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let _ = (receiver_id, amount, memo, msg);
        env::panic_str(&Refusal::new(RefusalCode::TransferCall).to_string())
    }

    /// NEP-145: makes an empty record for `account_id` (default: the caller) for the storage
    /// minimum and refunds the rest. An account that has a record gets the whole deposit back.
    #[payable]
    pub fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        // One record has one fixed size, so `registration_only` changes nothing.
        let _ = registration_only;
        let amount = env::attached_deposit();
        let account_id = account_id.unwrap_or_else(env::predecessor_account_id);
        let refund = if self.holders.contains_key(account_id.as_str()) {
            log!("The account is already registered, refunding the deposit");
            amount
        } else {
            let min = self.storage_balance_bounds().min;
            if amount < min {
                env::panic_str(&Refusal::new(RefusalCode::StorageDepositBelowMinimum).to_string());
            }
            self.holders.insert(
                account_id.to_string(),
                Holder::registered(min.as_yoctonear()),
            );
            amount.saturating_sub(min)
        };
        if !refund.is_zero() {
            Promise::new(env::predecessor_account_id())
                .transfer(refund)
                .detach();
        }
        self.storage_balance_of(account_id)
            .unwrap_or_else(|| env::panic_str("storage record missing"))
    }

    /// NEP-145: nothing is ever available to withdraw (`max` equals `min`).
    #[payable]
    pub fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        let balance = self
            .storage_balance_of(account_id.clone())
            .unwrap_or_else(|| {
                env::panic_str(&rule_engine::storage_withdraw_not_registered(
                    account_id.as_str(),
                ))
            });
        if amount.is_some_and(|a| !a.is_zero()) {
            env::panic_str(&Refusal::new(RefusalCode::StorageWithdrawAboveAvailable).to_string());
        }
        balance
    }

    /// NEP-145: removes the caller's record and refunds its storage deposit. With or without
    /// force, refused with `FC_UNREGISTER_WITH_BALANCE` while the account holds tokens, and with
    /// `FC_UNREGISTER_WINDOW_OPEN` at zero tokens while it has used allowance or a real NEAR cap
    /// record in the current window. Returns false when the caller has no record.
    #[payable]
    pub fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        let holder = self.holders.get(account_id.as_str());
        let unregistered =
            rule_engine::unregister(&self.state, holder, force.unwrap_or(false), now())
                .unwrap_or_else(|r| env::panic_str(&r.to_string()));
        match unregistered {
            Unregistered::NotRegistered => {
                log!("The account {} is not registered", account_id);
                false
            }
            Unregistered::Removed { refund } => {
                self.holders.remove(account_id.as_str());
                if refund > 0 {
                    Promise::new(account_id)
                        .transfer(NearToken::from_yoctonear(refund))
                        .detach();
                }
                true
            }
        }
    }

    pub fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        let record_bytes = RECORD_OVERHEAD_BYTES
            + HOLDERS_PREFIX.len() as u64
            + 4
            + LONGEST_ACCOUNT_ID_BYTES
            + near_sdk::borsh::to_vec(&Holder::default())
                .unwrap_or_default()
                .len() as u64;
        let min = env::storage_byte_cost().saturating_mul(record_bytes.into());
        StorageBalanceBounds {
            min,
            max: Some(min),
        }
    }

    /// NEP-145: `total` is the storage deposit the record holds (0 for a record a buy made).
    pub fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.holders
            .get(account_id.as_str())
            .map(|h| StorageBalance {
                total: NearToken::from_yoctonear(h.storage_deposit),
                available: NearToken::from_yoctonear(0),
            })
    }

    pub fn ft_total_supply(&self) -> U128 {
        U128(self.state.fixed.total_supply)
    }

    pub fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        U128(
            self.holders
                .get(account_id.as_str())
                .map_or(0, |h| h.balance),
        )
    }

    pub fn ft_metadata(&self) -> FungibleTokenMetadata {
        let metadata = &self.state.metadata;
        FungibleTokenMetadata {
            spec: "ft-1.0.0".into(),
            name: metadata.name.clone(),
            symbol: metadata.symbol.clone(),
            icon: metadata.icon.clone(),
            reference: None,
            reference_hash: None,
            decimals: self.state.fixed.decimals,
        }
    }

    pub fn fc_rules(&self) -> RulesView {
        rule_engine::view_rules(&self.state)
    }

    pub fn fc_pool(&self) -> PoolView {
        rule_engine::view_pool(&self.state, now())
    }

    pub fn fc_early_window(&self) -> EarlyWindowView {
        rule_engine::view_early_window(&self.state, now())
    }

    pub fn fc_cap(&self) -> CapView {
        rule_engine::view_cap(&self.state, now())
    }

    pub fn fc_account(&self, account_id: AccountId) -> AccountView {
        rule_engine::view_account(
            &self.state,
            account_id.as_str(),
            self.holders.get(account_id.as_str()),
            now(),
        )
    }

    pub fn fc_quote_sell(&self, account_id: AccountId, amount: U128) -> Option<SellQuote> {
        let holders = self.load(&[account_id.to_string()]);
        rule_engine::quote_sell(&self.state, holders, account_id.as_str(), amount.0, now())
    }

    pub fn fc_quote_buy(&self, account_id: AccountId, near_in: U128) -> Option<BuyQuote> {
        let holders = self.load(&[account_id.to_string()]);
        rule_engine::quote_buy(&self.state, holders, account_id.as_str(), near_in.0, now())
    }

    pub fn fc_creator_fees(&self) -> CreatorFeesView {
        rule_engine::view_creator_fees(&self.state, now())
    }
}

impl Coin {
    fn load(&self, accounts: &[String]) -> Holders {
        accounts
            .iter()
            .filter_map(|a| self.holders.get(a).map(|h| (a.clone(), h.clone())))
            .collect()
    }

    /// Run one rule step: panic on refusal, else store, log and pay.
    fn commit(&mut self, action: &Action) -> Option<Promise> {
        self.run(action).map(|p| {
            let to: AccountId = p
                .account
                .parse()
                .unwrap_or_else(|_| env::panic_str("bad payout account"));
            Promise::new(to).transfer(NearToken::from_yoctonear(p.amount))
        })
    }

    /// Run one rule step: panic on refusal, else store and log. Returns the NEAR to send.
    fn run(&mut self, action: &Action) -> Option<Payout> {
        let holders = self.load(&action.accounts());
        let Applied {
            state,
            holders,
            events,
            payout,
            ..
        } = rule_engine::apply(&self.state, holders, action, now())
            .unwrap_or_else(|r| env::panic_str(&r.to_string()));
        self.state = state;
        for (account, holder) in holders {
            self.holders.insert(account, holder);
        }
        for event in &events {
            env::log_str(&event.log_line());
        }
        payout
    }
}
