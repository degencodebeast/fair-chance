use std::fmt;

/// Hard refusal codes from INTERFACE.md. The contract panics with [`Refusal`]'s display string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            Self::SendToSelf => "Sender and receiver should be different",
            Self::AmountNotPositive => "The amount should be a positive number",
            Self::NotRegistered => "The account is not registered",
            Self::NotEnoughBalance => "The account doesn't have enough balance",
            Self::NotParentAccount => "new must be called by the parent account of the coin",
            Self::IconNotSmallWebp => {
                "icon must be a WebP data URL (data:image/webp;base64,...) under 8192 bytes"
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
