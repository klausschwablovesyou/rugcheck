use bytemuck::Pod;
use solana_sdk::pubkey::Pubkey;
use spl_token_2022::{
    extension::{
        default_account_state::DefaultAccountState,
        permanent_delegate::PermanentDelegate,
        transfer_fee::{TransferFee, TransferFeeConfig},
        transfer_hook::TransferHook,
        Extension, ExtensionType,
    },
    state::AccountState,
};

use crate::types::{ExtensionsCheck, RiskLevel};

/// Transfer fee (in basis points) from which the token is treated as a trap.
const DANGEROUS_TRANSFER_FEE_BPS: u16 = 1_000;

/// Extension type ids added to the Token-2022 program after the release of
/// the `spl-token-2022` crate this tool links against.
const CONFIDENTIAL_MINT_BURN: u16 = 24;
const SCALED_UI_AMOUNT: u16 = 25;
const PAUSABLE: u16 = 26;

/// One entry of a mint's extension data: `(extension type id, value)`.
pub type ExtensionEntry<'a> = (u16, &'a [u8]);

/// Splits the extension (TLV) area of a mint into its entries. Each entry is
/// a little-endian `u16` type, a `u16` length and the value. Returns `None`
/// if an entry runs past the end of the data.
///
/// This is done by hand because the library's own walk stops with an error at
/// the first type id it does not know, hiding every extension behind it.
pub fn extension_entries(tlv_data: &[u8]) -> Option<Vec<ExtensionEntry<'_>>> {
    let mut entries = Vec::new();
    let mut rest = tlv_data;
    while let Some((header, body)) = rest.split_first_chunk::<4>() {
        let type_id = u16::from_le_bytes([header[0], header[1]]);
        if type_id == u16::from(ExtensionType::Uninitialized) {
            break; // nothing is written after the first unused slot
        }
        let len = usize::from(u16::from_le_bytes([header[2], header[3]]));
        entries.push((type_id, body.get(..len)?));
        rest = &body[len..];
    }
    Some(entries)
}

fn extension_name(type_id: u16) -> String {
    match (ExtensionType::try_from(type_id), type_id) {
        (Ok(extension), _) => format!("{extension:?}"),
        (_, CONFIDENTIAL_MINT_BURN) => "ConfidentialMintBurn".to_string(),
        (_, SCALED_UI_AMOUNT) => "ScaledUiAmount".to_string(),
        (_, PAUSABLE) => "Pausable".to_string(),
        _ => format!("Unknown({type_id})"),
    }
}

/// Decodes extension `T` if the mint has it. An entry that is present but
/// does not decode is recorded in `undecodable` so it cannot pass as absent.
fn read_extension<'a, T: Extension + Pod>(
    entries: &[ExtensionEntry<'a>],
    undecodable: &mut Vec<String>,
) -> Option<&'a T> {
    let (type_id, value) = entries.iter().find(|(id, _)| *id == u16::from(T::TYPE))?;
    let extension = bytemuck::try_from_bytes(value).ok();
    if extension.is_none() {
        undecodable.push(extension_name(*type_id));
    }
    extension
}

fn optional_pubkey(bytes: &[u8]) -> Option<Pubkey> {
    Pubkey::try_from(bytes).ok().filter(|key| *key != Pubkey::default())
}

fn percent(basis_points: u16) -> f64 {
    f64::from(basis_points) / 100.0
}

/// Inspects the Token-2022 extensions of a mint (given its raw extension
/// data) for the ones that let the issuer take, block or tax holders' tokens.
/// Returns `None` for mints without extensions, which includes every legacy
/// SPL Token mint.
///
/// `current_epoch` decides which of the two scheduled transfer fees applies.
pub fn check_extensions(tlv_data: &[u8], current_epoch: Option<u64>) -> Option<ExtensionsCheck> {
    let Some(entries) = extension_entries(tlv_data) else {
        return Some(ExtensionsCheck {
            extensions: Vec::new(),
            risk: RiskLevel::Warning,
            risk_factors: vec!["Token-2022 extension data is malformed - verify manually".to_string()],
        });
    };
    if entries.is_empty() {
        return None;
    }

    let mut risk = RiskLevel::Safe;
    let mut risk_factors = Vec::new();
    let mut flag = |level: RiskLevel, factor: String| {
        risk = risk.max(level);
        risk_factors.push(factor);
    };
    let mut undecodable = Vec::new();

    if let Some(extension) = read_extension::<PermanentDelegate>(&entries, &mut undecodable) {
        if let Some(delegate) = Option::<Pubkey>::from(extension.delegate) {
            flag(
                RiskLevel::Danger,
                format!("Permanent delegate {delegate} can transfer or burn tokens from any account"),
            );
        }
    }

    if let Some(config) = read_extension::<TransferFeeConfig>(&entries, &mut undecodable) {
        // A fee capped at zero tokens is never taken, whatever its rate.
        let effective_bps = |fee: &TransferFee| {
            if u64::from(fee.maximum_fee) == 0 {
                0
            } else {
                u16::from(fee.transfer_fee_basis_points)
            }
        };
        let fee_level = |bps: u16| {
            if bps >= DANGEROUS_TRANSFER_FEE_BPS {
                RiskLevel::Danger
            } else {
                RiskLevel::Warning
            }
        };

        // The older fee applies until the newer one's epoch, and then stays
        // in the account as history. Without the current epoch, assume the
        // newer fee is already in force.
        let newer_fee_epoch = u64::from(config.newer_transfer_fee.epoch);
        let newer_fee_pending = current_epoch.is_some_and(|epoch| epoch < newer_fee_epoch);
        let newer_bps = effective_bps(&config.newer_transfer_fee);
        let current_bps = if newer_fee_pending {
            effective_bps(&config.older_transfer_fee)
        } else {
            newer_bps
        };

        if current_bps > 0 {
            flag(
                fee_level(current_bps),
                format!("Transfer fee of {}% is charged on every transfer", percent(current_bps)),
            );
        }
        if newer_fee_pending && newer_bps > 0 && newer_bps != current_bps {
            flag(
                fee_level(newer_bps),
                format!("Transfer fee changes to {}% from epoch {newer_fee_epoch}", percent(newer_bps)),
            );
        }
        if Option::<Pubkey>::from(config.transfer_fee_config_authority).is_some() {
            flag(
                RiskLevel::Warning,
                "Transfer fee authority is active - the fee can be raised".to_string(),
            );
        }
    }

    if let Some(hook) = read_extension::<TransferHook>(&entries, &mut undecodable) {
        if let Some(program) = Option::<Pubkey>::from(hook.program_id) {
            flag(
                RiskLevel::Warning,
                format!("Transfer hook program {program} runs on every transfer and can block it"),
            );
        }
        if Option::<Pubkey>::from(hook.authority).is_some() {
            flag(
                RiskLevel::Warning,
                "Transfer hook authority is active - a hook program can be installed or replaced".to_string(),
            );
        }
    }

    if let Some(default_state) = read_extension::<DefaultAccountState>(&entries, &mut undecodable) {
        if default_state.state == AccountState::Frozen as u8 {
            flag(
                RiskLevel::Danger,
                "New token accounts are frozen by default".to_string(),
            );
        }
    }

    for &(type_id, value) in &entries {
        match (ExtensionType::try_from(type_id), type_id) {
            (Ok(ExtensionType::NonTransferable), _) => {
                flag(RiskLevel::Danger, "Token is non-transferable".to_string());
            }
            (Ok(ExtensionType::ConfidentialTransferMint), _) => flag(
                RiskLevel::Warning,
                "Confidential transfers are enabled - confidential balances are invisible to the holder check"
                    .to_string(),
            ),
            (Ok(_), _) => {}
            // Pausable config: the pause authority (32 bytes), then a flag.
            (Err(_), PAUSABLE) if value.len() == 33 => {
                if value[32] != 0 {
                    flag(RiskLevel::Danger, "Transfers are currently paused".to_string());
                }
                if let Some(authority) = optional_pubkey(&value[..32]) {
                    flag(
                        RiskLevel::Danger,
                        format!("Pause authority {authority} can halt all transfers"),
                    );
                }
            }
            // Unknown to this tool: it must not count as harmless.
            (Err(_), _) => flag(
                RiskLevel::Warning,
                format!(
                    "Token-2022 extension {} is not analysed by this tool - verify manually",
                    extension_name(type_id)
                ),
            ),
        }
    }

    for name in undecodable {
        flag(
            RiskLevel::Warning,
            format!("Token-2022 extension {name} could not be decoded - verify manually"),
        );
    }

    Some(ExtensionsCheck {
        extensions: entries.iter().map(|&(type_id, _)| extension_name(type_id)).collect(),
        risk,
        risk_factors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::program_pack::Pack;
    use spl_token_2022::{
        extension::{BaseStateWithExtensions, StateWithExtensions, StateWithExtensionsMut},
        state::Mint,
    };

    /// Builds the account data of an initialized Token-2022 mint that has the
    /// given extensions allocated, letting `init` fill them in.
    fn mint_data(
        extensions: &[ExtensionType],
        init: impl FnOnce(&mut StateWithExtensionsMut<Mint>),
    ) -> Vec<u8> {
        let len = ExtensionType::try_calculate_account_len::<Mint>(extensions).unwrap();
        let mut data = vec![0; len];
        let mut state = StateWithExtensionsMut::<Mint>::unpack_uninitialized(&mut data).unwrap();
        init(&mut state);
        state.base = Mint {
            is_initialized: true,
            ..Mint::default()
        };
        state.pack_base();
        state.init_account_type().unwrap();
        data
    }

    /// Runs the check the way `run_all_checks` does, on full account data.
    fn check_at_epoch(data: &[u8], epoch: Option<u64>) -> Option<ExtensionsCheck> {
        let mint = StateWithExtensions::<Mint>::unpack(data).unwrap();
        check_extensions(mint.get_tlv_data(), epoch)
    }

    fn check(data: &[u8]) -> Option<ExtensionsCheck> {
        check_at_epoch(data, Some(100))
    }

    fn tlv_entry(type_id: u16, value: &[u8]) -> Vec<u8> {
        let mut entry = type_id.to_le_bytes().to_vec();
        entry.extend_from_slice(&(value.len() as u16).to_le_bytes());
        entry.extend_from_slice(value);
        entry
    }

    #[test]
    fn legacy_mint_has_no_extensions() {
        let mut data = vec![0; Mint::LEN];
        Mint {
            is_initialized: true,
            ..Mint::default()
        }
        .pack_into_slice(&mut data);
        assert!(check(&data).is_none());
    }

    #[test]
    fn splits_entries_and_stops_at_the_first_unused_slot() {
        let mut tlv = tlv_entry(12, &[1; 32]);
        tlv.extend(tlv_entry(9, &[]));
        tlv.extend_from_slice(&[0; 7]); // unused, zeroed tail
        assert_eq!(
            extension_entries(&tlv).unwrap(),
            [(12, [1; 32].as_slice()), (9, [].as_slice())]
        );
        assert_eq!(extension_entries(&[]).unwrap(), []);

        // A value that claims to be longer than the data that is left.
        let mut truncated = tlv_entry(12, &[1; 32]);
        truncated.truncate(20);
        assert!(extension_entries(&truncated).is_none());
        let result = check_extensions(&truncated, None).unwrap();
        assert_eq!(result.risk, RiskLevel::Warning);
        assert!(result.risk_factors[0].contains("malformed"));
    }

    #[test]
    fn permanent_delegate_is_danger() {
        let delegate = Pubkey::new_unique();
        let data = mint_data(&[ExtensionType::PermanentDelegate], |state| {
            state.init_extension::<PermanentDelegate>(true).unwrap().delegate =
                Some(delegate).try_into().unwrap();
        });
        let result = check(&data).unwrap();
        assert_eq!(result.extensions, ["PermanentDelegate"]);
        assert_eq!(result.risk, RiskLevel::Danger);
        assert_eq!(result.risk_factors.len(), 1);
        assert!(result.risk_factors[0].contains(&delegate.to_string()));
    }

    #[test]
    fn a_later_warning_does_not_lower_a_danger() {
        let data = mint_data(
            &[ExtensionType::PermanentDelegate, ExtensionType::TransferHook],
            |state| {
                state.init_extension::<PermanentDelegate>(true).unwrap().delegate =
                    Some(Pubkey::new_unique()).try_into().unwrap();
                state.init_extension::<TransferHook>(true).unwrap().program_id =
                    Some(Pubkey::new_unique()).try_into().unwrap();
            },
        );
        let result = check(&data).unwrap();
        assert_eq!(result.extensions, ["PermanentDelegate", "TransferHook"]);
        assert_eq!(result.risk, RiskLevel::Danger);
        assert_eq!(result.risk_factors.len(), 2);
    }

    #[test]
    fn unset_permanent_delegate_is_safe() {
        let data = mint_data(&[ExtensionType::PermanentDelegate], |state| {
            state.init_extension::<PermanentDelegate>(true).unwrap();
        });
        let result = check(&data).unwrap();
        assert_eq!(result.extensions, ["PermanentDelegate"]);
        assert_eq!(result.risk, RiskLevel::Safe);
        assert!(result.risk_factors.is_empty());
    }

    /// (basis points, maximum fee, first epoch)
    type Fee = (u16, u64, u64);

    fn transfer_fee(older: Fee, newer: Fee, authority: Option<Pubkey>, epoch: Option<u64>) -> ExtensionsCheck {
        let data = mint_data(&[ExtensionType::TransferFeeConfig], |state| {
            let config = state.init_extension::<TransferFeeConfig>(true).unwrap();
            for (fee, (bps, maximum, first_epoch)) in [
                (&mut config.older_transfer_fee, older),
                (&mut config.newer_transfer_fee, newer),
            ] {
                fee.transfer_fee_basis_points = bps.into();
                fee.maximum_fee = maximum.into();
                fee.epoch = first_epoch.into();
            }
            config.transfer_fee_config_authority = authority.try_into().unwrap();
        });
        check_at_epoch(&data, epoch).unwrap()
    }

    #[test]
    fn transfer_fee_levels() {
        let none = transfer_fee((0, 0, 0), (0, 0, 0), None, Some(100));
        assert_eq!(none.risk, RiskLevel::Safe);
        assert!(none.risk_factors.is_empty());

        let small = transfer_fee((150, 1_000, 0), (150, 1_000, 0), None, Some(100));
        assert_eq!(small.risk, RiskLevel::Warning);
        assert_eq!(small.risk_factors, ["Transfer fee of 1.5% is charged on every transfer"]);

        let just_below = transfer_fee((999, 1, 0), (999, 1, 0), None, Some(100));
        assert_eq!(just_below.risk, RiskLevel::Warning);
        let at_threshold = transfer_fee((1_000, 1, 0), (1_000, 1, 0), None, Some(100));
        assert_eq!(at_threshold.risk, RiskLevel::Danger);
        assert_eq!(at_threshold.risk_factors, ["Transfer fee of 10% is charged on every transfer"]);
    }

    #[test]
    fn only_the_fee_in_force_is_reported_as_charged() {
        // A 50% launch fee that was replaced by 0% in epoch 10 is history.
        let removed = transfer_fee((5_000, u64::MAX, 0), (0, 0, 10), None, Some(100));
        assert_eq!(removed.risk, RiskLevel::Safe);
        assert!(removed.risk_factors.is_empty());

        // The switch happens exactly at the newer fee's epoch.
        let at_switch = transfer_fee((0, 0, 0), (300, 9, 100), None, Some(100));
        assert_eq!(at_switch.risk_factors, ["Transfer fee of 3% is charged on every transfer"]);

        // One epoch earlier the older fee still applies and the newer one is announced.
        let scheduled = transfer_fee((100, 9, 0), (2_000, 9, 100), None, Some(99));
        assert_eq!(scheduled.risk, RiskLevel::Danger);
        assert_eq!(
            scheduled.risk_factors,
            [
                "Transfer fee of 1% is charged on every transfer",
                "Transfer fee changes to 20% from epoch 100",
            ]
        );

        // A scheduled removal is not a risk; the current fee still is.
        let ending = transfer_fee((100, 9, 0), (0, 0, 100), None, Some(99));
        assert_eq!(ending.risk_factors, ["Transfer fee of 1% is charged on every transfer"]);

        // Unknown epoch: assume the newer fee.
        let unknown_epoch = transfer_fee((100, 9, 0), (2_000, 9, 100), None, None);
        assert_eq!(unknown_epoch.risk_factors, ["Transfer fee of 20% is charged on every transfer"]);
    }

    #[test]
    fn fee_capped_at_zero_tokens_is_not_a_fee() {
        let capped = transfer_fee((5_000, 0, 0), (5_000, 0, 0), None, Some(100));
        assert_eq!(capped.risk, RiskLevel::Safe);
        assert!(capped.risk_factors.is_empty());
    }

    #[test]
    fn active_fee_authority_is_a_warning() {
        let changeable = transfer_fee((0, 0, 0), (0, 0, 0), Some(Pubkey::new_unique()), Some(100));
        assert_eq!(changeable.risk, RiskLevel::Warning);
        assert_eq!(
            changeable.risk_factors,
            ["Transfer fee authority is active - the fee can be raised"]
        );
    }

    fn transfer_hook(authority: Option<Pubkey>, program: Option<Pubkey>) -> ExtensionsCheck {
        let data = mint_data(&[ExtensionType::TransferHook], |state| {
            let hook = state.init_extension::<TransferHook>(true).unwrap();
            hook.authority = authority.try_into().unwrap();
            hook.program_id = program.try_into().unwrap();
        });
        check(&data).unwrap()
    }

    #[test]
    fn transfer_hook_program_and_authority_are_flagged_separately() {
        let program = Pubkey::new_unique();

        let inert = transfer_hook(None, None);
        assert_eq!(inert.risk, RiskLevel::Safe);
        assert!(inert.risk_factors.is_empty());

        let fixed_program = transfer_hook(None, Some(program));
        assert_eq!(fixed_program.risk, RiskLevel::Warning);
        assert_eq!(fixed_program.risk_factors.len(), 1);
        assert!(fixed_program.risk_factors[0].contains(&program.to_string()));

        // No program yet, but one can be installed at any time.
        let armed = transfer_hook(Some(Pubkey::new_unique()), None);
        assert_eq!(armed.risk, RiskLevel::Warning);
        assert_eq!(armed.risk_factors.len(), 1);
        assert!(armed.risk_factors[0].contains("authority is active"));

        assert_eq!(transfer_hook(Some(Pubkey::new_unique()), Some(program)).risk_factors.len(), 2);
    }

    fn default_account_state(state_value: AccountState) -> ExtensionsCheck {
        let data = mint_data(&[ExtensionType::DefaultAccountState], |state| {
            state.init_extension::<DefaultAccountState>(true).unwrap().state = state_value as u8;
        });
        check(&data).unwrap()
    }

    #[test]
    fn only_a_frozen_default_account_state_is_danger() {
        let frozen = default_account_state(AccountState::Frozen);
        assert_eq!(frozen.risk, RiskLevel::Danger);
        assert_eq!(frozen.risk_factors, ["New token accounts are frozen by default"]);

        let initialized = default_account_state(AccountState::Initialized);
        assert_eq!(initialized.risk, RiskLevel::Safe);
        assert!(initialized.risk_factors.is_empty());
    }

    #[test]
    fn non_transferable_is_danger() {
        let data = mint_data(&[ExtensionType::NonTransferable], |state| {
            state
                .init_extension::<spl_token_2022::extension::non_transferable::NonTransferable>(true)
                .unwrap();
        });
        let result = check(&data).unwrap();
        assert_eq!(result.risk, RiskLevel::Danger);
        assert_eq!(result.risk_factors, ["Token is non-transferable"]);
    }

    #[test]
    fn confidential_transfers_are_a_warning() {
        let tlv = tlv_entry(ExtensionType::ConfidentialTransferMint.into(), &[0; 65]);
        let result = check_extensions(&tlv, None).unwrap();
        assert_eq!(result.extensions, ["ConfidentialTransferMint"]);
        assert_eq!(result.risk, RiskLevel::Warning);
        assert_eq!(result.risk_factors.len(), 1);
    }

    #[test]
    fn harmless_extensions_are_listed_without_a_finding() {
        let data = mint_data(
            &[ExtensionType::MintCloseAuthority, ExtensionType::MetadataPointer],
            |state| {
                state
                    .init_extension::<spl_token_2022::extension::mint_close_authority::MintCloseAuthority>(true)
                    .unwrap();
                state
                    .init_extension::<spl_token_2022::extension::metadata_pointer::MetadataPointer>(true)
                    .unwrap();
            },
        );
        let result = check(&data).unwrap();
        assert_eq!(result.extensions, ["MintCloseAuthority", "MetadataPointer"]);
        assert_eq!(result.risk, RiskLevel::Safe);
        assert!(result.risk_factors.is_empty());
    }

    #[test]
    fn unknown_extension_does_not_hide_the_ones_around_it() {
        let delegate = Pubkey::new_unique();
        let mut tlv = tlv_entry(9_999, &[7; 5]);
        tlv.extend(tlv_entry(ExtensionType::PermanentDelegate.into(), delegate.as_ref()));
        tlv.extend(tlv_entry(SCALED_UI_AMOUNT, &[0; 56]));

        let result = check_extensions(&tlv, None).unwrap();
        assert_eq!(result.extensions, ["Unknown(9999)", "PermanentDelegate", "ScaledUiAmount"]);
        assert_eq!(result.risk, RiskLevel::Danger);
        assert_eq!(result.risk_factors.len(), 3);
        assert!(result.risk_factors[0].contains(&delegate.to_string()));
        assert!(result.risk_factors[1].contains("Unknown(9999) is not analysed"));
        assert!(result.risk_factors[2].contains("ScaledUiAmount is not analysed"));
    }

    #[test]
    fn unknown_extension_alone_is_never_safe() {
        let result = check_extensions(&tlv_entry(9_999, &[]), None).unwrap();
        assert_eq!(result.risk, RiskLevel::Warning);
        assert_eq!(result.risk_factors.len(), 1);
    }

    #[test]
    fn pausable_token() {
        let authority = Pubkey::new_unique();
        let pausable = |authority: &[u8], paused: u8| {
            let mut value = authority.to_vec();
            value.push(paused);
            check_extensions(&tlv_entry(PAUSABLE, &value), None).unwrap()
        };

        let armed = pausable(authority.as_ref(), 0);
        assert_eq!(armed.extensions, ["Pausable"]);
        assert_eq!(armed.risk, RiskLevel::Danger);
        assert_eq!(armed.risk_factors, [format!("Pause authority {authority} can halt all transfers")]);

        let paused = pausable(authority.as_ref(), 1);
        assert_eq!(paused.risk_factors.len(), 2);
        assert_eq!(paused.risk_factors[0], "Transfers are currently paused");

        // Authority renounced while not paused: nothing can pause it any more.
        let renounced = pausable(&[0; 32], 0);
        assert_eq!(renounced.risk, RiskLevel::Safe);
        assert!(renounced.risk_factors.is_empty());

        // A layout this tool does not recognise falls back to "not analysed".
        let odd = check_extensions(&tlv_entry(PAUSABLE, &[0; 40]), None).unwrap();
        assert_eq!(odd.risk, RiskLevel::Warning);
        assert!(odd.risk_factors[0].contains("Pausable is not analysed"));
    }

    #[test]
    fn known_extension_with_a_wrong_size_is_not_treated_as_absent() {
        let tlv = tlv_entry(ExtensionType::PermanentDelegate.into(), &[1; 31]);
        let result = check_extensions(&tlv, None).unwrap();
        assert_eq!(result.risk, RiskLevel::Warning);
        assert_eq!(
            result.risk_factors,
            ["Token-2022 extension PermanentDelegate could not be decoded - verify manually"]
        );
    }
}
