use std::fmt;

/// Hard refusal codes from INTERFACE.md. The contract panics with [`Refusal`]'s display string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum RefusalCode {
    OneYocto,
    Slippage,
    TransferCall,
    LargeCannotSend,
    /// The detail is the largest amount the sender can send to this receiver.
    ReceiverWouldBeLarge,
    SendToCoin,
    /// `storage_unregister` while the account holds tokens, with or without force.
    UnregisterWithBalance,
    /// `storage_unregister` with no tokens, but with used allowance or a real NEAR cap record
    /// in the current window, with or without force.
    UnregisterWindowOpen,
    /// A buy by the creator group, or a send into it, that takes the group above
    /// `creator_cap_bps` of supply.
    CreatorCap,
    /// A sell of marked tokens in blocks 0 to N.
    EarlyWindow,
    /// Not an `FC_` code: the standard NEP-141 error for a send to yourself.
    SendToSelf,
    /// Not an `FC_` code: the standard NEP-141 error for a send of 0 tokens.
    AmountNotPositive,
    /// Not an `FC_` code: the NEP-145 error for a send to an account with no storage record. The
    /// detail is the account.
    NotRegistered,
    /// Not an `FC_` code: the standard NEP-141 balance error.
    NotEnoughBalance,
    /// Not an `FC_` code: `new` called by an account that is not the coin's parent (ADR 0006:
    /// only the factory starts a coin).
    NotParentAccount,
    /// Not an `FC_` code: the NEP-148 icon is not a WebP data URL under 8 KB.
    IconNotSmallWebp,
    /// Not an `FC_` code: the NEP-145 error for a `storage_deposit` below `storage_balance_bounds.min`
    /// from an account with no record.
    StorageDepositBelowMinimum,
    /// Not an `FC_` code: the NEP-145 error for a `storage_withdraw` of more than the available
    /// storage balance (always 0 here).
    StorageWithdrawAboveAvailable,
}

impl RefusalCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OneYocto => "FC_ONE_YOCTO",
            Self::Slippage => "FC_SLIPPAGE",
            Self::TransferCall => "FC_TRANSFER_CALL",
            Self::LargeCannotSend => "FC_LARGE_CANNOT_SEND",
            Self::ReceiverWouldBeLarge => "FC_RECEIVER_WOULD_BE_LARGE",
            Self::SendToCoin => "FC_SEND_TO_COIN",
            Self::UnregisterWithBalance => "FC_UNREGISTER_WITH_BALANCE",
            Self::UnregisterWindowOpen => "FC_UNREGISTER_WINDOW_OPEN",
            Self::CreatorCap => "FC_CREATOR_CAP",
            Self::EarlyWindow => "FC_EARLY_WINDOW",
            Self::SendToSelf => "Sender and receiver should be different",
            Self::AmountNotPositive => "The amount should be a positive number",
            Self::NotRegistered => "The account is not registered",
            Self::NotEnoughBalance => "The account doesn't have enough balance",
            Self::NotParentAccount => "new must be called by the parent account of the coin",
            Self::IconNotSmallWebp => {
                "icon must be a WebP data URL (data:image/webp;base64,...) under 8192 bytes"
            }
            Self::StorageDepositBelowMinimum => {
                "The attached deposit is less than the minimum storage balance"
            }
            Self::StorageWithdrawAboveAvailable => {
                "The amount is greater than the available storage balance"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: RefusalCode,
    pub detail: Option<String>,
}

impl Refusal {
    pub fn new(code: RefusalCode) -> Self {
        Self { code, detail: None }
    }

    pub fn with_detail(code: RefusalCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: Some(detail.into()),
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.detail {
            Some(detail) => write!(f, "{}: {}", self.code.as_str(), detail),
            None => f.write_str(self.code.as_str()),
        }
    }
}

impl std::error::Error for Refusal {}

/// The other NEP-145 "not registered" form, which `storage_withdraw` returns: the account sits
/// inside the text, not after a colon as in [`RefusalCode::NotRegistered`].
pub fn storage_withdraw_not_registered(account: &str) -> String {
    format!("The account {account} is not registered")
}

#[cfg(test)]
mod every_refusal_message_fixture {
    use super::*;
    use strum::IntoEnumIterator;

    /// The shared fixture the Go and TypeScript refusal-text readers test against.
    const FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../refusal-texts/every-refusal-message.json"
    );
    const SAMPLE_ACCOUNT: &str = "sample-buyer.test.near";

    /// The detail a refusal carries, or `None`. No wildcard arm: a new code must say.
    fn sample_detail(code: RefusalCode) -> Option<&'static str> {
        match code {
            RefusalCode::NotRegistered => Some(SAMPLE_ACCOUNT),
            RefusalCode::ReceiverWouldBeLarge => Some("12500000000000000000000000"),
            RefusalCode::OneYocto
            | RefusalCode::Slippage
            | RefusalCode::TransferCall
            | RefusalCode::LargeCannotSend
            | RefusalCode::SendToCoin
            | RefusalCode::UnregisterWithBalance
            | RefusalCode::UnregisterWindowOpen
            | RefusalCode::CreatorCap
            | RefusalCode::EarlyWindow
            | RefusalCode::SendToSelf
            | RefusalCode::AmountNotPositive
            | RefusalCode::NotEnoughBalance
            | RefusalCode::NotParentAccount
            | RefusalCode::IconNotSmallWebp
            | RefusalCode::StorageDepositBelowMinimum
            | RefusalCode::StorageWithdrawAboveAvailable => None,
        }
    }

    /// A struct, not a JSON map, so the field order does not depend on which serde_json features
    /// the build turns on.
    #[derive(serde::Serialize)]
    struct ContractRefusal {
        refusal: String,
        message: String,
    }

    fn every_refusal_message() -> String {
        let mut entries: Vec<ContractRefusal> = RefusalCode::iter()
            .map(|code| {
                let refusal = match sample_detail(code) {
                    Some(detail) => Refusal::with_detail(code, detail),
                    None => Refusal::new(code),
                };
                ContractRefusal {
                    refusal: format!("{code:?}"),
                    message: refusal.to_string(),
                }
            })
            .collect();
        entries.push(ContractRefusal {
            refusal: "NotRegistered (storage_withdraw form)".to_string(),
            message: storage_withdraw_not_registered(SAMPLE_ACCOUNT),
        });
        serde_json::to_string_pretty(&entries).expect("json") + "\n"
    }

    /// Writes the fixture when it is out of date, and then fails, so the new file gets committed.
    #[test]
    fn the_shared_fixture_lists_every_refusal_message_the_contract_can_return() {
        let want = every_refusal_message();
        let have = std::fs::read_to_string(FIXTURE).unwrap_or_default();
        if have != want {
            std::fs::write(FIXTURE, &want).expect("write the fixture");
            panic!("{FIXTURE} was out of date; it is now rewritten, so commit it");
        }
    }

    #[test]
    fn the_two_not_registered_forms_name_the_account() {
        assert_eq!(
            Refusal::with_detail(RefusalCode::NotRegistered, "bob.near").to_string(),
            "The account is not registered: bob.near"
        );
        assert_eq!(
            storage_withdraw_not_registered("bob.near"),
            "The account bob.near is not registered"
        );
    }
}
