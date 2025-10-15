use anchor_lang::error_code;

#[error_code]
pub enum EscrowError {
    #[msg("Escrow locked. Try again after lock period elapses")]
    EscrowLocked,

    #[msg("UnknownError")]
    UnknownError,
}
    