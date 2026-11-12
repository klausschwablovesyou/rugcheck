use solana_sdk::{account::Account, pubkey, pubkey::Pubkey};
use spl_token_2022::extension::ExtensionType;

use super::extensions::extension_entries;
use crate::types::TokenMetadata;

const METADATA_PROGRAM_ID: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

/// Discriminator (`Key::MetadataV1`) in the first byte of a metadata account.
const METADATA_V1_KEY: u8 = 4;

/// Derives the address of the Metaplex metadata account of a mint.
pub fn metadata_pda(mint_address: &Pubkey) -> Pubkey {
    let seeds = &[
        b"metadata",
        METADATA_PROGRAM_ID.as_ref(),
        mint_address.as_ref(),
    ];
    Pubkey::find_program_address(seeds, &METADATA_PROGRAM_ID).0
}

/// Parses a Metaplex metadata account. Returns `None` if the account is not a
/// metadata account or its data is malformed.
pub fn parse_metaplex_account(account: &Account) -> Option<TokenMetadata> {
    if account.owner != METADATA_PROGRAM_ID {
        return None;
    }
    parse_metadata_data(&account.data)
}

/// Token-2022 mints can carry their metadata in the mint account itself, as
/// the `TokenMetadata` extension. Takes the mint's raw extension data.
pub fn from_mint_extension(tlv_data: &[u8]) -> Option<TokenMetadata> {
    let (_, value) = extension_entries(tlv_data)?
        .into_iter()
        .find(|(type_id, _)| *type_id == u16::from(ExtensionType::TokenMetadata))?;

    // Extension layout: update authority (32 bytes), mint (32 bytes), then
    // the name, symbol and URI as Borsh strings.
    read_strings(value.get(32 + 32..)?)
}

fn parse_metadata_data(data: &[u8]) -> Option<TokenMetadata> {
    // Metadata account layout (only the prefix this tool needs):
    // - 1 byte: key
    // - 32 bytes: update authority
    // - 32 bytes: mint
    // - 4 bytes: name length + name (padded with NULs to 32)
    // - 4 bytes: symbol length + symbol (padded to 10)
    // - 4 bytes: uri length + uri (padded to 200)
    if data.first() != Some(&METADATA_V1_KEY) {
        return None;
    }

    read_strings(data.get(1 + 32 + 32..)?)
}

/// Reads the consecutive name, symbol and URI strings.
fn read_strings(mut data: &[u8]) -> Option<TokenMetadata> {
    Some(TokenMetadata {
        name: read_string(&mut data)?,
        symbol: read_string(&mut data)?,
        uri: read_string(&mut data)?,
    })
}

/// Reads a Borsh string (u32 little-endian length prefix) and advances `data`
/// past it. Returns `None` if the data is truncated.
fn read_string(data: &mut &[u8]) -> Option<String> {
    let (len, rest) = data.split_first_chunk::<4>()?;
    let len = usize::try_from(u32::from_le_bytes(*len)).ok()?;
    let bytes = rest.get(..len)?;
    *data = &rest[len..];

    Some(sanitize(&String::from_utf8_lossy(bytes)))
}

/// Token creators control these strings and they end up on the terminal, so
/// drop everything that could rewrite the report: control characters (NUL
/// padding, ANSI escapes, newlines) and bidirectional overrides.
fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            !c.is_control()
                && !matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn borsh_string(s: &str, padded_len: usize) -> Vec<u8> {
        let mut bytes = (padded_len as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(s.as_bytes());
        bytes.resize(4 + padded_len, 0);
        bytes
    }

    fn metadata_account_data(name: &str, symbol: &str, uri: &str) -> Vec<u8> {
        let mut data = vec![METADATA_V1_KEY];
        data.extend_from_slice(&[7; 64]); // update authority + mint
        data.extend(borsh_string(name, 32));
        data.extend(borsh_string(symbol, 10));
        data.extend(borsh_string(uri, 200));
        data.extend_from_slice(&[0; 100]); // remaining fields, ignored
        data
    }

    #[test]
    fn derives_the_known_usdc_metadata_address() {
        let usdc = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
        assert_eq!(
            metadata_pda(&usdc),
            pubkey!("5x38Kp4hvdomTCnCrAny4UtMUt5rQBdB6px2K1Ui45Wq")
        );
    }

    #[test]
    fn parses_padded_fields() {
        let data = metadata_account_data("USD Coin", "USDC", "https://example.com/usdc.json");
        let metadata = parse_metadata_data(&data).unwrap();
        assert_eq!(metadata.name, "USD Coin");
        assert_eq!(metadata.symbol, "USDC");
        assert_eq!(metadata.uri, "https://example.com/usdc.json");
    }

    #[test]
    fn accepts_a_metaplex_owned_v1_account() {
        // Literals on purpose: the fixtures above reuse the constants under test.
        let mut data = metadata_account_data("USD Coin", "USDC", "");
        data[0] = 4;
        let account = Account {
            data,
            owner: pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"),
            ..Account::default()
        };
        let metadata = parse_metaplex_account(&account).unwrap();
        assert_eq!(metadata.name, "USD Coin");
        assert_eq!(metadata.symbol, "USDC");
        assert_eq!(metadata.uri, "");
    }

    #[test]
    fn parsed_strings_are_sanitized() {
        let data = metadata_account_data("\x1b[2JSAFE\u{202E}", "S\nYM", "https://x\r/y");
        let metadata = parse_metadata_data(&data).unwrap();
        assert_eq!(metadata.name, "[2JSAFE");
        assert_eq!(metadata.symbol, "SYM");
        assert_eq!(metadata.uri, "https://x/y");
    }

    #[test]
    fn invalid_utf8_is_replaced_not_rejected() {
        let mut data = metadata_account_data("abc", "SYM", "");
        data[1 + 64 + 4] = 0xFF; // first byte of the name
        assert_eq!(parse_metadata_data(&data).unwrap().name, "\u{FFFD}bc");
    }

    #[test]
    fn rejects_accounts_that_are_not_metadata() {
        let mut data = metadata_account_data("Name", "SYM", "");
        data[0] = 6; // Key::MasterEditionV2
        assert!(parse_metadata_data(&data).is_none());
        assert!(parse_metadata_data(&[]).is_none());

        let account = Account {
            data: metadata_account_data("Name", "SYM", ""),
            owner: Pubkey::new_unique(),
            ..Account::default()
        };
        assert!(parse_metaplex_account(&account).is_none());
    }

    #[test]
    fn rejects_truncated_data() {
        let data = metadata_account_data("Name", "SYM", "uri");
        // Cut inside the symbol, inside a length prefix, and inside the header.
        for len in [1 + 64 + 36 + 6, 1 + 64 + 2, 40] {
            assert!(parse_metadata_data(&data[..len]).is_none(), "len {len}");
        }

        // A length prefix pointing past the end of the data.
        let mut data = vec![METADATA_V1_KEY];
        data.extend_from_slice(&[0; 64]);
        data.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_metadata_data(&data).is_none());
    }

    #[test]
    fn reads_token_2022_metadata_extension_behind_an_unknown_extension() {
        let entry = |type_id: u16, value: &[u8]| {
            let mut entry = type_id.to_le_bytes().to_vec();
            entry.extend_from_slice(&(value.len() as u16).to_le_bytes());
            entry.extend_from_slice(value);
            entry
        };
        let mut value = vec![9; 64]; // update authority + mint
        for field in ["Tesla xStock\x1b[2J", "TSLAx", "https://example.com/tslax.json"] {
            value.extend_from_slice(&(field.len() as u32).to_le_bytes());
            value.extend_from_slice(field.as_bytes());
        }
        value.extend_from_slice(&0u32.to_le_bytes()); // no additional metadata

        let mut tlv = entry(9_999, &[1, 2, 3]);
        tlv.extend(entry(ExtensionType::TokenMetadata.into(), &value));

        let metadata = from_mint_extension(&tlv).unwrap();
        assert_eq!(metadata.name, "Tesla xStock[2J");
        assert_eq!(metadata.symbol, "TSLAx");
        assert_eq!(metadata.uri, "https://example.com/tslax.json");

        assert!(from_mint_extension(&entry(9_999, &[1, 2, 3])).is_none());
        assert!(from_mint_extension(&[]).is_none());
        // Extension present but cut short.
        assert!(from_mint_extension(&entry(ExtensionType::TokenMetadata.into(), &value[..70])).is_none());
    }

    #[test]
    fn strips_terminal_control_sequences() {
        assert_eq!(sanitize("Coin\0\0\0"), "Coin");
        assert_eq!(sanitize("\x1b[2J\x1b[1;32mSAFE\x1b[0m"), "[2J[1;32mSAFE[0m");
        assert_eq!(sanitize("line1\r\nline2"), "line1line2");
        assert_eq!(sanitize("abc\u{202E}def"), "abcdef");
        assert_eq!(sanitize("  Ünïcödé 🚀 "), "Ünïcödé 🚀");
    }

    #[test]
    fn strips_every_bidi_control() {
        for c in "\u{200E}\u{200F}\u{202A}\u{202B}\u{202C}\u{202D}\u{202E}\u{2066}\u{2067}\u{2068}\u{2069}".chars() {
            assert_eq!(sanitize(&format!("a{c}b")), "ab", "U+{:04X}", c as u32);
        }
    }
}
