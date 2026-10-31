use crate::{
    live::{PendingOrder, RealAccount, ThbRate},
    market::{SOL_MINT, USDC_MINT},
    solana_wire::{self as wire, Packet},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub async fn rpc(client: &reqwest::Client, method: &str, params: Value) -> Result<Value, String> {
    let url = std::env::var("CRYPTO_BROS_RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".into());
    let response = client
        .post(url)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .map_err(|_| "Solana RPC is unreachable; check the private RPC setting")?;
    if !response.status().is_success() {
        return Err(format!(
            "Solana RPC returned HTTP {}; a dedicated RPC endpoint may be needed",
            response.status().as_u16()
        ));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|_| "Invalid Solana RPC response")?;
    if value.get("error").is_some() {
        return Err("Solana RPC rejected the request".into());
    }
    value
        .get("result")
        .cloned()
        .ok_or("Solana RPC returned no result".into())
}
pub async fn mainnet(client: &reqwest::Client) -> Result<(), String> {
    if rpc(client, "getGenesisHash", json!([])).await?.as_str()
        != Some("5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d")
    {
        return Err(
            "Live trading requires Solana mainnet; the configured RPC points elsewhere".into(),
        );
    }
    Ok(())
}
pub async fn rate(client: &reqwest::Client) -> Result<ThbRate, String> {
    let response = client
        .get("https://api.coinbase.com/v2/exchange-rates?currency=USDC")
        .send()
        .await
        .map_err(|_| "THB valuation source is unavailable")?;
    if !response.status().is_success() {
        return Err("THB valuation source rejected the request".into());
    }
    let value: Value = response
        .json()
        .await
        .map_err(|_| "Invalid THB valuation response")?;
    if value["data"]["currency"] != "USDC" {
        return Err("Unexpected valuation currency".into());
    }
    let rate = value["data"]["rates"]["THB"]
        .as_str()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0 && *v < 10_000.0)
        .ok_or("Missing USDC/THB valuation")?;
    Ok(ThbRate {
        micros: (rate * 1_000_000.0).round() as u64,
        fetched_at: chrono::Utc::now().timestamp(),
        source: "Coinbase USDC/THB reference; fetched time, not a cash-out quote".into(),
    })
}
fn decoded_account(value: &Value) -> Result<Vec<u8>, String> {
    STANDARD
        .decode(value["data"][0].as_str().ok_or("Missing account data")?)
        .map_err(|_| "Invalid account data".into())
}
pub async fn accounts(
    client: &reqwest::Client,
    addresses: &[String],
) -> Result<Vec<Value>, String> {
    let result = rpc(
        client,
        "getMultipleAccounts",
        json!([addresses,{"encoding":"base64","commitment":"confirmed"}]),
    )
    .await?;
    let accounts = result["value"]
        .as_array()
        .ok_or("Missing account balances")?;
    if accounts.len() != addresses.len() {
        return Err("Incomplete account balance response".into());
    }
    Ok(accounts.clone())
}
fn u64_at(data: &[u8], offset: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        data.get(offset..offset + 8)
            .ok_or("Truncated account")?
            .try_into()
            .map_err(|_| "Invalid account integer")?,
    ))
}
fn token(value: &Value, owner: &str, mint: &str) -> Result<(u64, u64), String> {
    if value.is_null() {
        return Ok((0, 0));
    }
    // Callers request the wallet's canonical ATAs. After WSOL CloseAccount, RPC
    // simulation may return this empty System account instead of null. No token
    // balance or rent remains. Never extend this to funded/data-bearing accounts.
    if mint == SOL_MINT
        && value["owner"] == wire::SYSTEM
        && value["lamports"].as_u64() == Some(0)
        && value["executable"] == false
        && value["data"] == json!(["", "base64"])
        && value.get("space").is_none_or(|v| v.as_u64() == Some(0))
    {
        return Ok((0, 0));
    }
    if value["owner"] != wire::TOKEN || value["executable"] == true {
        return Err("Unexpected token account program".into());
    }
    let data = decoded_account(value)?;
    if data.len() != 165
        || data[..32] != wire::key(mint)?
        || data[32..64] != wire::key(owner)?
        || data[108] != 1
        || data[72..76] != [0; 4]
        || data[129..133] != [0; 4]
    {
        return Err(
            "Token account has an unexpected mint, owner, delegate, authority or frozen state"
                .into(),
        );
    }
    let amount = u64_at(&data, 64)?;
    let lamports = value["lamports"]
        .as_u64()
        .ok_or("Missing account lamports")?;
    let rent = if mint == SOL_MINT {
        lamports
            .checked_sub(amount)
            .ok_or("Invalid wrapped SOL balance")?
    } else {
        lamports
    };
    Ok((amount, rent))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balances {
    pub sol: u64,
    pub usdc: u64,
    pub wrapped_sol: u64,
    pub rent: u64,
    pub checked_at: i64,
}
pub fn balances_from(values: &[Value], owner: &str) -> Result<Balances, String> {
    if values.len() != 3
        || values[0].is_null()
        || values[0]["owner"] != wire::SYSTEM
        || values[0]["executable"] == true
    {
        return Err("A funded, ordinary Phantom SOL account is required".into());
    }
    let (usdc, usdc_rent) = token(&values[1], owner, USDC_MINT)?;
    let (wrapped_sol, sol_rent) = token(&values[2], owner, SOL_MINT)?;
    Ok(Balances {
        sol: values[0]["lamports"]
            .as_u64()
            .ok_or("Missing SOL balance")?,
        usdc,
        wrapped_sol,
        rent: usdc_rent.checked_add(sol_rent).ok_or("Rent overflow")?,
        checked_at: chrono::Utc::now().timestamp(),
    })
}
pub fn wallet_accounts(owner: &str) -> Result<Vec<String>, String> {
    wire::key(owner)?;
    Ok(vec![
        owner.into(),
        wire::ata(owner, USDC_MINT)?,
        wire::ata(owner, SOL_MINT)?,
    ])
}
pub async fn balances(client: &reqwest::Client, owner: &str) -> Result<Balances, String> {
    mainnet(client).await?;
    balances_from(&accounts(client, &wallet_accounts(owner)?).await?, owner)
}
pub async fn resolve(client: &reqwest::Client, packet: &Packet) -> Result<Vec<String>, String> {
    let mut keys = packet.keys.clone();
    if packet.lookups.is_empty() {
        return Ok(keys);
    }
    let values = accounts(
        client,
        &packet
            .lookups
            .iter()
            .map(|v| v.address.clone())
            .collect::<Vec<_>>(),
    )
    .await?;
    let mut writable = vec![];
    let mut readonly = vec![];
    for (table, value) in packet.lookups.iter().zip(values.iter()) {
        if value["owner"] != wire::LOOKUP {
            return Err("Invalid address lookup table owner".into());
        }
        let data = decoded_account(value)?;
        if data.len() < 56
            || (data.len() - 56) % 32 != 0
            || data[..4] != 1_u32.to_le_bytes()
            || u64_at(&data, 4)? != u64::MAX
        {
            return Err("Inactive or malformed address lookup table".into());
        }
        for (indices, destination) in [
            (&table.writable, &mut writable),
            (&table.readonly, &mut readonly),
        ] {
            for index in indices {
                destination.push(
                    bs58::encode(
                        data.get(56 + 32 * (*index as usize)..88 + 32 * (*index as usize))
                            .ok_or("Invalid lookup index")?,
                    )
                    .into_string(),
                );
            }
        }
    }
    keys.extend(writable);
    keys.extend(readonly);
    if keys.len() > 64 {
        return Err("Route exceeds the account limit".into());
    }
    Ok(keys)
}
pub fn execution_deltas(
    before: &Balances,
    after: &Balances,
    fee: u64,
    side: &str,
) -> Result<(u64, u64, i64), String> {
    let rent = after.rent as i128 - before.rent as i128;
    let sol = after.sol as i128 - before.sol as i128 + after.wrapped_sol as i128
        - before.wrapped_sol as i128
        + rent
        + fee as i128;
    let usdc = after.usdc as i128 - before.usdc as i128;
    let (input, output) = if side == "buy" {
        (-usdc, sol)
    } else if side == "sell" {
        (-sol, usdc)
    } else {
        return Err("Invalid direction".into());
    };
    if input <= 0 || output <= 0 {
        return Err("Transaction does not exchange the approved assets".into());
    }
    Ok((
        u64::try_from(input).map_err(|_| "Input overflow")?,
        u64::try_from(output).map_err(|_| "Output overflow")?,
        i64::try_from(rent).map_err(|_| "Rent overflow")?,
    ))
}
pub async fn inspect(
    client: &reqwest::Client,
    account: &RealAccount,
    order: &PendingOrder,
) -> Result<(u64, i64), String> {
    mainnet(client).await?;
    let packet = wire::decode(&order.unsigned_transaction)?;
    let keys = resolve(client, &packet).await?;
    wire::validate_instructions(&packet, &keys, &account.address, &order.side, order.input)?;
    let all = accounts(client, &keys).await?;
    let wallet_keys = wallet_accounts(&account.address)?;
    let owner_bytes = wire::key(&account.address)?;
    for (key, value) in keys.iter().zip(all.iter()) {
        if value["owner"] == wire::TOKEN || value["owner"] == wire::TOKEN_2022 {
            let data = decoded_account(value)?;
            if data.len() >= 64 && data[32..64] == owner_bytes && !wallet_keys.contains(key) {
                return Err("Route touches another token account owned by this wallet".into());
            }
        }
    }
    let before = balances_from(&accounts(client, &wallet_keys).await?, &account.address)
        .map_err(|e| format!("Before swap: {e}"))?;
    let tracked_sol = account.layers.iter().try_fold(0_u64, |sum, layer| {
        sum.checked_add(layer.quantity)
            .ok_or("Tracked SOL overflow")
    })?;
    if before.sol < tracked_sol.saturating_add(account.config.sol_gas_reserve) {
        return Err(
            "Native SOL does not cover the tracked positions plus the separate fee reserve".into(),
        );
    }
    if before.usdc < account.cash {
        return Err(
            "Wallet USDC is below the tracked balance; reconcile external withdrawals first".into(),
        );
    }
    // Existing wrapped SOL can change wrapping semantics; require a plain native SOL wallet.
    if before.wrapped_sol != 0 {
        return Err("Unwrap existing wrapped SOL before using this release".into());
    }
    let fee = rpc(
        client,
        "getFeeForMessage",
        json!([STANDARD.encode(&packet.message),{"commitment":"confirmed"}]),
    )
    .await?["value"]
        .as_u64()
        .ok_or("Quote blockhash expired; prepare again")?;
    if fee > account.config.max_network_fee {
        return Err("Network fee exceeds the configured cap".into());
    }
    let simulation=rpc(client,"simulateTransaction",json!([order.unsigned_transaction,{"encoding":"base64","sigVerify":false,"commitment":"confirmed","accounts":{"encoding":"base64","addresses":wallet_keys}}])).await?;
    if !simulation["value"]["err"].is_null() {
        return Err(
            "Solana simulation rejected this route; check SOL fees, balances and try a fresh quote"
                .into(),
        );
    }
    let after = balances_from(
        simulation["value"]["accounts"]
            .as_array()
            .ok_or("Simulation did not return account balances")?,
        &account.address,
    )
    .map_err(|e| format!("After swap simulation: {e}"))?;
    let (input, output, rent) = execution_deltas(&before, &after, fee, &order.side)?;
    let next_tracked_sol = if order.side == "buy" {
        tracked_sol.checked_add(output)
    } else {
        tracked_sol.checked_sub(input)
    }
    .ok_or("Tracked SOL overflow")?;
    if input != order.input
        || output < order.minimum_output
        || after.sol < next_tracked_sol.saturating_add(account.config.sol_gas_reserve)
    {
        return Err(
            "Simulated spending, minimum receipt or SOL reserve violates the approved limits"
                .into(),
        );
    }
    Ok((fee, rent))
}
fn amount(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| value[field].as_u64())
        .ok_or_else(|| format!("Missing quote {field}"))
}
pub async fn prepare(
    client: &reqwest::Client,
    account: &RealAccount,
    side: &str,
    input: u64,
    layer_id: u64,
    price: u64,
    rate: ThbRate,
    target: u64,
    review: u64,
    model: String,
) -> Result<PendingOrder, String> {
    let (input_mint, output_mint) = if side == "buy" {
        (USDC_MINT, SOL_MINT)
    } else {
        (SOL_MINT, USDC_MINT)
    };
    let mut request = client.get("https://api.jup.ag/swap/v2/order").query(&[
        ("inputMint", input_mint.to_string()),
        ("outputMint", output_mint.to_string()),
        ("amount", input.to_string()),
        ("taker", account.address.clone()),
        ("slippageBps", account.config.slippage_bps.to_string()),
        ("excludeRouters", "jupiterz,dflow,okx".into()),
        ("maxSupportedTransactionVersion", "0".into()),
    ]);
    if let Ok(key) = std::env::var("CRYPTO_BROS_JUPITER_API_KEY") {
        if !key.trim().is_empty() {
            request = request.header("x-api-key", key);
        }
    }
    let response = request
        .send()
        .await
        .map_err(|_| "Jupiter order request failed")?;
    if !response.status().is_success() {
        return Err(format!(
            "Jupiter order request returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let quote: Value = response
        .json()
        .await
        .map_err(|_| "Invalid Jupiter order response")?;
    if quote["router"] != "metis"
        || quote["swapMode"] != "ExactIn"
        || quote["inputMint"] != input_mint
        || quote["outputMint"] != output_mint
        || amount(&quote, "inAmount")? != input
        || quote["slippageBps"].as_u64() != Some(account.config.slippage_bps as u64)
        || quote["gasless"] == true
        || quote["errorCode"].as_i64().is_some_and(|v| v != 0)
    {
        return Err("Jupiter returned an unsupported route or a different order".into());
    }
    let output = amount(&quote, "outAmount")?;
    let minimum = amount(&quote, "otherAmountThreshold")?;
    let fair_output = if side == "buy" {
        crate::live::ratio(input, crate::live::SOL, price)?
    } else {
        crate::live::ratio(input, price, crate::live::SOL)?
    };
    if minimum as u128 * 100 < fair_output as u128 * 98 {
        return Err("The executable quote is over 2% below the current reference value; refresh prices or wait for a better route".into());
    }
    if minimum == 0
        || minimum > output
        || minimum
            < (output as u128 * (10_000 - account.config.slippage_bps as u128) / 10_000) as u64
    {
        return Err("Quote minimum output exceeds the allowed slippage".into());
    }
    let now = chrono::Utc::now().timestamp();
    let mut order = PendingOrder {
        entry_context: None,
        id: uuid::Uuid::new_v4().to_string(),
        side: side.into(),
        layer_id,
        input,
        expected_output: output,
        minimum_output: minimum,
        unsigned_transaction: quote["transaction"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Jupiter could not prepare a funded-wallet transaction")?
            .into(),
        request_id: quote["requestId"]
            .as_str()
            .ok_or("Missing Jupiter request identifier")?
            .into(),
        last_valid_block_height: amount(&quote, "lastValidBlockHeight")?,
        prepared_at: now,
        expires_at: now + 60,
        signature: None,
        status: "awaiting_approval".into(),
        network_fee: 0,
        rent_delta: 0,
        rate,
        price,
        target_price: target,
        review_price: review,
        model,
        message: None,
    };
    let (fee, rent) = inspect(client, account, &order).await?;
    order.network_fee = fee;
    order.rent_delta = rent;
    Ok(order)
}
/// This function is reached only after an exact, verified Phantom signature is durably saved.
pub async fn execute(
    client: &reqwest::Client,
    request_id: &str,
    signed: &str,
) -> Result<(), String> {
    let mut request = client
        .post("https://api.jup.ag/swap/v2/execute")
        .json(&json!({"signedTransaction":signed,"requestId":request_id}));
    if let Ok(key) = std::env::var("CRYPTO_BROS_JUPITER_API_KEY") {
        if !key.trim().is_empty() {
            request = request.header("x-api-key", key);
        }
    }
    let response=request.send().await.map_err(|_|"Submission response was interrupted; checking the saved signature before any new order")?;
    if !response.status().is_success() {
        return Err(
            "Submission returned an error; the order remains pending until chain reconciliation"
                .into(),
        );
    }
    // Even Jupiter's success response is not a ledger fill. Only finalized chain data can book it.
    Ok(())
}

pub enum Settlement {
    Pending,
    Expired,
    Failed {
        fee: u64,
        time: i64,
    },
    Filled {
        input: u64,
        output: u64,
        fee: u64,
        rent: i64,
        time: i64,
    },
}
fn transaction_balance(
    meta: &Value,
    keys: &[String],
    owner: &str,
    prefix: &str,
) -> Result<Balances, String> {
    let native = meta[format!("{prefix}Balances")]
        .as_array()
        .ok_or("Missing finalized native balances")?;
    let tokens = meta[format!("{prefix}TokenBalances")]
        .as_array()
        .ok_or("Missing finalized token balances")?;
    let owned = wallet_accounts(owner)?;
    let mut amounts = [0_u64; 2];
    let mut rent = 0_u64;
    for (n, address) in owned.iter().skip(1).enumerate() {
        let index = keys
            .iter()
            .position(|key| key == address)
            .ok_or("Swap omitted a required associated token account")?;
        if let Some(token) = tokens
            .iter()
            .find(|t| t["accountIndex"].as_u64() == Some(index as u64))
        {
            let mint = if n == 0 { USDC_MINT } else { SOL_MINT };
            if token["owner"] != owner || token["mint"] != mint {
                return Err("Finalized token ownership differs from the approved wallet".into());
            }
            amounts[n] = token["uiTokenAmount"]["amount"]
                .as_str()
                .and_then(|v| v.parse().ok())
                .ok_or("Invalid finalized token amount")?;
        }
        let lamports = native
            .get(index)
            .and_then(Value::as_u64)
            .ok_or("Missing finalized account balance")?;
        let locked = if n == 1 {
            lamports
                .checked_sub(amounts[n])
                .ok_or("Invalid finalized wrapped SOL rent")?
        } else {
            lamports
        };
        rent = rent.checked_add(locked).ok_or("Rent overflow")?;
    }
    Ok(Balances {
        sol: native
            .first()
            .and_then(Value::as_u64)
            .ok_or("Missing fee payer balance")?,
        usdc: amounts[0],
        wrapped_sol: amounts[1],
        rent,
        checked_at: 0,
    })
}
pub async fn settlement(
    client: &reqwest::Client,
    owner: &str,
    order: &PendingOrder,
) -> Result<Settlement, String> {
    let signature = order
        .signature
        .as_ref()
        .ok_or("Order has not been signed")?;
    mainnet(client).await?;
    let result=rpc(client,"getTransaction",json!([signature,{"encoding":"base64","commitment":"finalized","maxSupportedTransactionVersion":0}])).await?;
    if result.is_null() {
        let statuses = rpc(
            client,
            "getSignatureStatuses",
            json!([[signature],{"searchTransactionHistory":true}]),
        )
        .await?;
        let statuses = statuses["value"]
            .as_array()
            .filter(|s| s.len() == 1)
            .ok_or("Invalid signature-status response")?;
        if !statuses[0].is_null() {
            return Ok(Settlement::Pending);
        }
        if chrono::Utc::now().timestamp() - order.prepared_at > 3600 {
            return Err("An older submission is missing from this RPC's history. Keep it pending and use a history-capable RPC to reconcile; do not repeat the order".into());
        }
        let height = rpc(
            client,
            "getBlockHeight",
            json!([{"commitment":"finalized"}]),
        )
        .await?
        .as_u64()
        .ok_or("Missing finalized block height")?;
        return Ok(if height > order.last_valid_block_height {
            Settlement::Expired
        } else {
            Settlement::Pending
        });
    }
    decode_settlement(owner, order, &result)
}
fn decode_settlement(
    owner: &str,
    order: &PendingOrder,
    result: &Value,
) -> Result<Settlement, String> {
    let signature = order.signature.as_ref().ok_or("Missing saved signature")?;
    let signed = result["transaction"][0]
        .as_str()
        .ok_or("Missing finalized transaction bytes")?;
    if wire::verify_signed(&order.unsigned_transaction, signed, owner)? != *signature {
        return Err("Finalized signature mismatch".into());
    }
    let packet = wire::decode(signed)?;
    let meta = &result["meta"];
    let fee = meta["fee"].as_u64().ok_or("Missing finalized fee")?;
    let time = result["blockTime"]
        .as_i64()
        .ok_or("Missing finalized block time")?;
    if !meta["err"].is_null() {
        return Ok(Settlement::Failed { fee, time });
    }
    let mut keys = packet.keys;
    for kind in ["writable", "readonly"] {
        if let Some(loaded) = meta["loadedAddresses"][kind].as_array() {
            for key in loaded {
                keys.push(
                    key.as_str()
                        .ok_or("Invalid finalized lookup address")?
                        .into(),
                );
            }
        }
    }
    let before = transaction_balance(meta, &keys, owner, "pre")?;
    let after = transaction_balance(meta, &keys, owner, "post")?;
    let (input, output, rent) = execution_deltas(&before, &after, fee, &order.side)?;
    Ok(Settlement::Filled {
        input,
        output,
        fee,
        rent,
        time,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::SOL;
    use ed25519_dalek::{Signer, SigningKey};
    use sha2::{Digest, Sha256};

    fn token_account_fixture(owner: &str, mint: &str, amount: u64) -> Value {
        let mut data = vec![0; 165];
        data[..32].copy_from_slice(&wire::key(mint).unwrap());
        data[32..64].copy_from_slice(&wire::key(owner).unwrap());
        data[64..72].copy_from_slice(&amount.to_le_bytes());
        data[108] = 1;
        json!({"owner":wire::TOKEN,"executable":false,"lamports":2_039_280,
            "data":[STANDARD.encode(data),"base64"],"space":165})
    }

    fn closed_wsol_fixture() -> Value {
        // SPL Token CloseAccount drains lamports, assigns System and resizes to zero.
        // Public account-state fixture only; no wallet key or signed transaction.
        json!({"owner":wire::SYSTEM,"executable":false,"lamports":0,
            "data":["","base64"],"space":0})
    }

    #[test]
    fn closed_wsol_simulation_preserves_buy_and_sell_deltas() {
        let owner = bs58::encode([7; 32]).into_string();
        let wallet = |sol| json!({"owner":wire::SYSTEM,"executable":false,"lamports":sol});
        let before = balances_from(
            &[
                wallet(SOL),
                token_account_fixture(&owner, USDC_MINT, 20_000_000),
                Value::Null,
            ],
            &owner,
        )
        .unwrap();
        for (side, sol, usdc, expected) in [
            (
                "buy",
                SOL + 100_000_000 - 5000,
                10_000_000,
                (10_000_000, 100_000_000, 0),
            ),
            (
                "sell",
                SOL - 100_000_000 - 5000,
                30_000_000,
                (100_000_000, 10_000_000, 0),
            ),
        ] {
            let after = balances_from(
                &[
                    wallet(sol),
                    token_account_fixture(&owner, USDC_MINT, usdc),
                    closed_wsol_fixture(),
                ],
                &owner,
            )
            .expect("closed WSOL is an empty account, not a foreign token program");
            assert_eq!(after.wrapped_sol, 0);
            assert_eq!(after.rent, before.rent);
            assert_eq!(
                execution_deltas(&before, &after, 5000, side).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn empty_wsol_exception_rejects_funds_data_wrong_program_and_missing_fields() {
        let owner = bs58::encode([7; 32]).into_string();
        for (field, value) in [
            ("owner", json!(wire::TOKEN_2022)),
            ("owner", json!(wire::JUPITER)),
            ("executable", json!(true)),
            ("lamports", json!(1)),
            ("data", json!([STANDARD.encode([0]), "base64"])),
            ("data", json!(["", "base58"])),
            ("space", json!(165)),
        ] {
            let mut malformed = closed_wsol_fixture();
            malformed[field] = value;
            assert!(token(&malformed, &owner, SOL_MINT).is_err(), "{field}");
        }
        for field in ["owner", "executable", "lamports", "data"] {
            let mut malformed = closed_wsol_fixture();
            malformed.as_object_mut().unwrap().remove(field);
            assert!(
                token(&malformed, &owner, SOL_MINT).is_err(),
                "missing {field}"
            );
        }
        assert!(token(&closed_wsol_fixture(), &owner, USDC_MINT).is_err());
        let mut no_space = closed_wsol_fixture();
        no_space.as_object_mut().unwrap().remove("space");
        assert_eq!(token(&no_space, &owner, SOL_MINT).unwrap(), (0, 0));
    }
    #[test]
    fn finalized_receipt_uses_exact_signed_bytes_and_atomic_balance_changes() {
        let seed: [u8; 32] = Sha256::digest(uuid::Uuid::new_v4().as_bytes()).into();
        let signer = SigningKey::from_bytes(&seed);
        let owner = bs58::encode(signer.verifying_key().to_bytes()).into_string();
        let mut message = vec![128, 1, 0, 1, 4];
        for address in [
            owner.clone(),
            wire::ata(&owner, USDC_MINT).unwrap(),
            wire::ata(&owner, SOL_MINT).unwrap(),
            wire::JUPITER.into(),
        ] {
            message.extend(wire::key(&address).unwrap());
        }
        message.extend([9; 32]);
        message.extend([1, 3, 3, 0, 1, 2, 8]);
        message.extend([0; 8]);
        message.push(0);
        let signature = signer.sign(&message).to_bytes();
        let mut unsigned = vec![1];
        unsigned.extend([0; 64]);
        unsigned.extend(&message);
        let mut signed = vec![1];
        signed.extend(signature);
        signed.extend(&message);
        let order:PendingOrder=serde_json::from_value(json!({"id":"synthetic","side":"buy","layer_id":0,"input":10000000,"expected_output":100000000,"minimum_output":99000000,"unsigned_transaction":STANDARD.encode(unsigned),"request_id":"synthetic","last_valid_block_height":100,"prepared_at":0,"expires_at":60,"signature":bs58::encode(signature).into_string(),"status":"pending","network_fee":5000,"rent_delta":0,"rate":{"micros":35000000,"fetched_at":0,"source":"synthetic"},"price":100000000,"target_price":110000000,"review_price":90000000,"model":"synthetic","message":null})).unwrap();
        let balance = |amount: &str| json!({"accountIndex":1,"mint":USDC_MINT,"owner":owner,"uiTokenAmount":{"amount":amount,"decimals":6}});
        let mut result = json!({"transaction":[STANDARD.encode(signed),"base64"],"blockTime":20,"meta":{"err":null,"fee":5000,"preBalances":[1000000000,2039280,0,0],"postBalances":[1099995000,2039280,0,0],"preTokenBalances":[balance("30000000")],"postTokenBalances":[balance("20000000")],"loadedAddresses":{"writable":[],"readonly":[]}}});
        match decode_settlement(&owner, &order, &result).unwrap() {
            Settlement::Filled {
                input,
                output,
                fee,
                rent,
                ..
            } => assert_eq!((input, output, fee, rent), (10000000, 100000000, 5000, 0)),
            _ => panic!("expected synthetic finalized receipt"),
        }
        result["meta"]["postTokenBalances"][0]["owner"] = json!(wire::SYSTEM);
        assert!(decode_settlement(&owner, &order, &result).is_err());
        result["meta"]["err"] = json!({"InstructionError":[0,"Custom"]});
        assert!(matches!(
            decode_settlement(&owner, &order, &result).unwrap(),
            Settlement::Failed { fee: 5000, .. }
        ));
    }
    #[test]
    fn rent_is_not_misreported_as_trading_loss_or_output() {
        let before = Balances {
            sol: SOL,
            usdc: 20_000_000,
            wrapped_sol: 0,
            rent: 0,
            checked_at: 0,
        };
        let after = Balances {
            sol: SOL + 100_000_000 - 5000 - 2_039_280,
            usdc: 10_000_000,
            wrapped_sol: 0,
            rent: 2_039_280,
            checked_at: 1,
        };
        assert_eq!(
            execution_deltas(&before, &after, 5000, "buy").unwrap(),
            (10_000_000, 100_000_000, 2_039_280)
        );
        assert!(execution_deltas(&before, &after, 5000, "sell").is_err());
    }
}
