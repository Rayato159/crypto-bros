//! Bounded, read-only decoding of legacy/v0 packets. Never creates wallet keys.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

pub const SYSTEM: &str = "11111111111111111111111111111111";
pub const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const ASSOCIATED: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
pub const COMPUTE: &str = "ComputeBudget111111111111111111111111111111";
pub const JUPITER: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
pub const LOOKUP: &str = "AddressLookupTab1e1111111111111111111111111";

pub fn key(text: &str) -> Result<[u8; 32], String> {
    bs58::decode(text)
        .into_vec()
        .map_err(|_| "Invalid public address")?
        .try_into()
        .map_err(|_| "Invalid public address length".into())
}
pub fn ata(owner: &str, mint: &str) -> Result<String, String> {
    let owner = key(owner)?;
    let mint = key(mint)?;
    let token = key(TOKEN)?;
    let program = key(ASSOCIATED)?;
    for bump in (0..=255_u8).rev() {
        let mut hash = Sha256::new();
        for bytes in [
            &owner[..],
            &token[..],
            &mint[..],
            &[bump],
            &program[..],
            b"ProgramDerivedAddress",
        ] {
            hash.update(bytes);
        }
        let bytes: [u8; 32] = hash.finalize().into();
        if VerifyingKey::from_bytes(&bytes).is_err() {
            return Ok(bs58::encode(bytes).into_string());
        }
    }
    Err("Cannot derive associated token address".into())
}
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).ok_or("Invalid packet length")?;
        let bytes = self
            .data
            .get(self.pos..end)
            .ok_or("Truncated transaction")?;
        self.pos = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.bytes(1)?[0])
    }
    fn short(&mut self) -> Result<usize, String> {
        let mut result = 0_usize;
        for i in 0..3 {
            let b = self.byte()?;
            if i == 2 && b > 3 {
                return Err("Invalid compact length".into());
            }
            result |= ((b & 127) as usize) << (7 * i);
            if b & 128 == 0 {
                if i > 0 && b == 0 {
                    return Err("Non-canonical compact length".into());
                }
                return Ok(result);
            }
        }
        Err("Invalid compact length".into())
    }
}
#[derive(Debug, Clone)]
pub struct Instruction {
    pub program: usize,
    pub accounts: Vec<usize>,
    pub data: Vec<u8>,
}
#[derive(Debug, Clone)]
pub struct Lookup {
    pub address: String,
    pub writable: Vec<u8>,
    pub readonly: Vec<u8>,
}
#[derive(Debug, Clone)]
pub struct Packet {
    pub signature: [u8; 64],
    pub message: Vec<u8>,
    pub keys: Vec<String>,
    pub instructions: Vec<Instruction>,
    pub lookups: Vec<Lookup>,
}
pub fn decode(encoded: &str) -> Result<Packet, String> {
    if encoded.len() > 1700 {
        return Err("Only bounded legacy/v0 transactions are supported".into());
    }
    let data = STANDARD
        .decode(encoded)
        .map_err(|_| "Invalid transaction encoding")?;
    if data.len() > 1232 {
        return Err("Transaction exceeds the v0 packet limit".into());
    }
    let mut r = Reader {
        data: &data,
        pos: 0,
    };
    if r.short()? != 1 {
        return Err("Only transactions signed by this wallet alone are supported; gasless/multisigner routes are disabled".into());
    }
    let signature = r
        .bytes(64)?
        .try_into()
        .map_err(|_| "Invalid signature size")?;
    let start = r.pos;
    let first = r.byte()?;
    let versioned = first & 128 != 0;
    if versioned && first != 128 {
        return Err("This release supports legacy/v0, not v1 transactions".into());
    }
    let signers = if versioned { r.byte()? } else { first };
    let readonly_signed = r.byte()?;
    let readonly_unsigned = r.byte()?;
    let count = r.short()?;
    if signers != 1
        || readonly_signed != 0
        || count == 0
        || count > 64
        || readonly_unsigned as usize >= count
    {
        return Err("Invalid signer or account header".into());
    }
    let mut keys = vec![];
    for _ in 0..count {
        keys.push(bs58::encode(r.bytes(32)?).into_string());
    }
    r.bytes(32)?; // recent blockhash remains part of the exact signed message
    let count = r.short()?;
    if count == 0 || count > 32 {
        return Err("Unexpected instruction count".into());
    }
    let mut instructions = vec![];
    for _ in 0..count {
        let program = r.byte()? as usize;
        let count = r.short()?;
        if count > 64 {
            return Err("Too many instruction accounts".into());
        }
        let accounts = r.bytes(count)?.iter().map(|v| *v as usize).collect();
        let count = r.short()?;
        let data = r.bytes(count)?.to_vec();
        instructions.push(Instruction {
            program,
            accounts,
            data,
        });
    }
    let mut lookups = vec![];
    if versioned {
        let count = r.short()?;
        if count > 16 {
            return Err("Too many lookup tables".into());
        }
        for _ in 0..count {
            let address = bs58::encode(r.bytes(32)?).into_string();
            let n = r.short()?;
            let writable = r.bytes(n)?.to_vec();
            let n = r.short()?;
            let readonly = r.bytes(n)?.to_vec();
            lookups.push(Lookup {
                address,
                writable,
                readonly,
            });
        }
    }
    if r.pos != data.len() {
        return Err("Trailing transaction data".into());
    }
    Ok(Packet {
        signature,
        message: data[start..].to_vec(),
        keys,
        instructions,
        lookups,
    })
}
pub fn verify_signed(unsigned: &str, signed: &str, owner: &str) -> Result<String, String> {
    let expected = decode(unsigned)?;
    let actual = decode(signed)?;
    if actual.message != expected.message || actual.keys.first().map(String::as_str) != Some(owner)
    {
        return Err("Signed transaction differs from the approved order".into());
    }
    VerifyingKey::from_bytes(&key(owner)?)
        .map_err(|_| "Invalid wallet signer")?
        .verify_strict(&actual.message, &Signature::from_bytes(&actual.signature))
        .map_err(|_| "Wallet signature did not verify")?;
    Ok(bs58::encode(actual.signature).into_string())
}
pub fn validate_instructions(
    packet: &Packet,
    keys: &[String],
    owner: &str,
    side: &str,
    input: u64,
) -> Result<(), String> {
    if packet.signature != [0; 64]
        || keys.first().map(String::as_str) != Some(owner)
        || keys.len() > 64
    {
        return Err(
            "The quote must contain an unsigned transaction paid for by the selected wallet".into(),
        );
    }
    let usdc = ata(owner, crate::market::USDC_MINT)?;
    let wsol = ata(owner, crate::market::SOL_MINT)?;
    let mut swaps = 0;
    let mut funded = 0_u64;
    for ix in &packet.instructions {
        let program = keys.get(ix.program).ok_or("Invalid program index")?;
        let accounts: Vec<_> = ix
            .accounts
            .iter()
            .map(|i| {
                keys.get(*i)
                    .map(String::as_str)
                    .ok_or("Invalid account index")
            })
            .collect::<Result<_, _>>()?;
        match program.as_str() {
            JUPITER => {
                swaps += 1;
                if ix.data.len() < 8 {
                    return Err("Malformed Jupiter instruction".into());
                }
            }
            COMPUTE => {
                if !accounts.is_empty()
                    || !matches!(
                        (ix.data.first(), ix.data.len()),
                        (Some(1 | 2 | 4), 5) | (Some(3), 9)
                    )
                {
                    return Err("Unexpected compute-budget instruction".into());
                }
            }
            ASSOCIATED => {
                if accounts.len() < 6
                    || accounts[0] != owner
                    || accounts[2] != owner
                    || accounts[4] != SYSTEM
                    || accounts[5] != TOKEN
                    || !(ix.data.is_empty() || ix.data == [1])
                    || !((accounts[1] == usdc && accounts[3] == crate::market::USDC_MINT)
                        || (accounts[1] == wsol && accounts[3] == crate::market::SOL_MINT))
                {
                    return Err("Quote creates an unexpected token account".into());
                }
            }
            SYSTEM => {
                if side != "sell"
                    || accounts != [owner, wsol.as_str()]
                    || ix.data.len() != 12
                    || ix.data[..4] != 2_u32.to_le_bytes()
                {
                    return Err("Unexpected SOL transfer or authority operation".into());
                }
                let amount =
                    u64::from_le_bytes(ix.data[4..12].try_into().map_err(|_| "Invalid transfer")?);
                funded = funded.checked_add(amount).ok_or("SOL transfer overflow")?;
                if funded > input {
                    return Err("SOL transfer exceeds approved sale".into());
                }
            }
            TOKEN => {
                let safe = (ix.data == [17] && accounts == [wsol.as_str()])
                    || (ix.data == [9] && accounts == [wsol.as_str(), owner, owner]);
                if !safe {
                    return Err(
                        "Token approvals, authority changes and unrelated transfers are forbidden"
                            .into(),
                    );
                }
            }
            _ => return Err("Quote contains an unsupported top-level program".into()),
        }
    }
    if swaps != 1 {
        return Err("Exactly one Jupiter swap is required".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    #[test]
    fn associated_address_matches_the_official_javascript_sdk() {
        assert_eq!(
            ata(SYSTEM, crate::market::USDC_MINT).unwrap(),
            "HJt8Tjdsc9ms9i4WCZEzhzr4oyf3ANcdzXrNdLPFqm3M"
        );
    }
    #[test]
    fn only_the_exact_message_and_wallet_signature_are_accepted() {
        // Ephemeral unfunded test material, generated in memory; never a user's wallet.
        let seed: [u8; 32] = Sha256::digest(uuid::Uuid::new_v4().as_bytes()).into();
        let signer = SigningKey::from_bytes(&seed);
        let owner = bs58::encode(signer.verifying_key().to_bytes()).into_string();
        let mut message = vec![128, 1, 0, 1, 2];
        message.extend(signer.verifying_key().to_bytes());
        message.extend(key(JUPITER).unwrap());
        message.extend([9; 32]);
        message.extend([1, 1, 0, 8]);
        message.extend([0; 8]);
        message.push(0);
        let mut unsigned = vec![1];
        unsigned.extend([0; 64]);
        unsigned.extend(&message);
        let mut signed = vec![1];
        signed.extend(signer.sign(&message).to_bytes());
        signed.extend(&message);
        assert!(
            verify_signed(
                &STANDARD.encode(&unsigned),
                &STANDARD.encode(&signed),
                &owner
            )
            .is_ok()
        );
        signed[2] ^= 1;
        assert!(
            verify_signed(
                &STANDARD.encode(&unsigned),
                &STANDARD.encode(&signed),
                &owner
            )
            .is_err()
        );
        signed[2] ^= 1;
        let mut changed = message.clone();
        changed[80] ^= 1;
        let mut changed_signed = vec![1];
        changed_signed.extend(signer.sign(&changed).to_bytes());
        changed_signed.extend(changed);
        assert!(
            verify_signed(
                &STANDARD.encode(&unsigned),
                &STANDARD.encode(&changed_signed),
                &owner
            )
            .is_err()
        );
        assert!(
            verify_signed(
                &STANDARD.encode(&unsigned),
                &STANDARD.encode(&signed),
                SYSTEM
            )
            .is_err()
        );
    }
    #[test]
    fn malformed_packets_never_panic() {
        for size in 0..200 {
            assert!(decode(&STANDARD.encode(vec![255; size])).is_err());
        }
        assert!(decode(&"A".repeat(1800)).is_err());
    }
    #[test]
    fn foreign_transfers_and_token_approvals_are_rejected() {
        let owner = bs58::encode([7; 32]).into_string();
        let mut packet = Packet {
            signature: [0; 64],
            message: vec![],
            keys: vec![owner.clone(), TOKEN.into(), JUPITER.into()],
            lookups: vec![],
            instructions: vec![
                Instruction {
                    program: 2,
                    accounts: vec![],
                    data: vec![0; 8],
                },
                Instruction {
                    program: 1,
                    accounts: vec![0],
                    data: vec![4],
                },
            ],
        };
        assert!(validate_instructions(&packet, &packet.keys, &owner, "buy", 1).is_err());
        packet.instructions.remove(1);
        assert!(validate_instructions(&packet, &packet.keys, &owner, "buy", 1).is_ok());
    }
}
