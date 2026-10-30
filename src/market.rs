use crate::state::{Quote, Wallet};
use crypto_bros_core::indicators::Candle;
use serde_json::{Value, json};

pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
pub const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

pub async fn candles(client: &reqwest::Client) -> Result<(Vec<Candle>, f64), String> {
    let response = client
        .get("https://data-api.binance.vision/api/v3/klines")
        .query(&[
            ("symbol", "SOLUSDC"),
            ("interval", "15m"),
            ("limit", "1000"),
        ])
        .send()
        .await
        .map_err(|_| "Market data connection failed")?;
    if !response.status().is_success() {
        return Err(format!(
            "Market provider returned HTTP {}. Try again later or use explicit demo mode.",
            response.status().as_u16()
        ));
    }
    let rows: Vec<Vec<Value>> = response
        .json()
        .await
        .map_err(|_| "Market provider returned invalid candles")?;
    let now = chrono::Utc::now().timestamp_millis();
    let mut output = vec![];
    let number = |v: Option<&Value>| {
        v.and_then(Value::as_str)
            .and_then(|s| s.parse::<f64>().ok())
    };
    for row in rows {
        if row
            .get(6)
            .and_then(Value::as_i64)
            .is_none_or(|end| end >= now)
        {
            continue;
        }
        let Some(time) = row.first().and_then(Value::as_i64) else {
            continue;
        };
        if let (Some(open), Some(high), Some(low), Some(close), Some(volume)) = (
            number(row.get(1)),
            number(row.get(2)),
            number(row.get(3)),
            number(row.get(4)),
            number(row.get(5)),
        ) {
            let candle = Candle {
                time: time / 1000,
                open,
                high,
                low,
                close,
                volume,
            };
            if candle.valid() {
                output.push(candle);
            }
        }
    }
    if output.len() < 100
        || output.windows(2).any(|w| w[1].time - w[0].time != 900)
        || output
            .last()
            .is_none_or(|c| !(900..1800).contains(&(now / 1000 - c.time)))
    {
        return Err("Incomplete or discontinuous candles; trading paused".into());
    }
    let ticker: Value = client
        .get("https://data-api.binance.vision/api/v3/ticker/price")
        .query(&[("symbol", "SOLUSDC")])
        .send()
        .await
        .map_err(|_| "Price connection failed")?
        .error_for_status()
        .map_err(|_| "Price provider rejected request")?
        .json()
        .await
        .map_err(|_| "Invalid price response")?;
    let price = number(ticker.get("price"))
        .filter(|p| p.is_finite() && *p > 0.0)
        .ok_or("Invalid market price")?;
    Ok((output, price))
}
pub async fn quote(
    client: &reqwest::Client,
    amount: u64,
    api_key: Option<&str>,
) -> Result<Quote, String> {
    let mut request = client.get("https://api.jup.ag/swap/v2/order").query(&[
        ("inputMint", USDC_MINT.to_string()),
        ("outputMint", SOL_MINT.to_string()),
        ("amount", amount.to_string()),
        ("slippageBps", "50".to_string()),
    ]);
    if let Some(key) = api_key {
        if !key.is_empty() {
            request = request.header("x-api-key", key);
        }
    }
    let response = request
        .send()
        .await
        .map_err(|_| "Jupiter connection failed")?;
    if !response.status().is_success() {
        return Err(format!(
            "Jupiter returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let result: Value = response
        .json()
        .await
        .map_err(|_| "Jupiter response is invalid")?;
    let sol_out = result
        .get("outAmount")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or("Jupiter quote has no output amount")? as f64
        / 1e9;
    if sol_out <= 0.0 {
        return Err("Jupiter returned zero output".into());
    }
    // No taker, signing, transaction construction, or submit endpoint in this build.
    let amount_usdc = amount as f64 / 1e6;
    Ok(Quote {
        amount_usdc,
        sol_out,
        effective_price: amount_usdc / sol_out,
        fee_bps: result.get("feeBps").and_then(Value::as_u64).unwrap_or(0),
        time: chrono::Utc::now().timestamp(),
        source: "Jupiter Swap V2 / quote only".into(),
    })
}
pub fn valid_address(address: &str) -> bool {
    (32..=44).contains(&address.len())
        && bs58::decode(address)
            .into_vec()
            .is_ok_and(|bytes| bytes.len() == 32)
}
pub async fn wallet(client: &reqwest::Client, address: &str) -> Result<Wallet, String> {
    if !valid_address(address) {
        return Err("A Solana public address must decode to 32 bytes".into());
    }
    let url = std::env::var("CRYPTO_BROS_RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".into());
    let body = json!([
        {"jsonrpc":"2.0","id":1,"method":"getBalance","params":[address,{"commitment":"confirmed"}]},
        {"jsonrpc":"2.0","id":2,"method":"getTokenAccountsByOwner","params":[address,{"mint":USDC_MINT},{"encoding":"jsonParsed","commitment":"confirmed"}]}
    ]);
    let results: Vec<Value> = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|_| "Solana RPC connection failed")?
        .error_for_status()
        .map_err(|_| "Solana RPC rejected request")?
        .json()
        .await
        .map_err(|_| "Invalid Solana RPC response")?;
    if results.iter().any(|r| r.get("error").is_some()) {
        return Err("Solana RPC returned an error; balance was not verified".into());
    }
    let balance = results
        .iter()
        .find(|r| r["id"] == 1)
        .and_then(|r| r["result"]["value"].as_u64())
        .ok_or("Missing SOL balance")?;
    let token_accounts = results
        .iter()
        .find(|r| r["id"] == 2)
        .and_then(|r| r["result"]["value"].as_array())
        .ok_or("Missing USDC accounts")?;
    let mut usdc = 0_u64;
    for account in token_accounts {
        let amount = account["account"]["data"]["parsed"]["info"]["tokenAmount"]["amount"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or("Invalid USDC balance")?;
        usdc = usdc.checked_add(amount).ok_or("Balance overflow")?;
    }
    Ok(Wallet {
        address: address.into(),
        sol: balance as f64 / 1e9,
        usdc: usdc as f64 / 1e6,
        verified_at: Some(chrono::Utc::now().timestamp()),
    })
}
pub async fn email(client: &reqwest::Client, message: &str) -> Result<(), String> {
    let key = std::env::var("CRYPTO_BROS_EMAIL_API_KEY").map_err(|_| "Email is not configured")?;
    let from = std::env::var("CRYPTO_BROS_EMAIL_FROM").map_err(|_| "Email sender is missing")?;
    let to = std::env::var("CRYPTO_BROS_EMAIL_TO").map_err(|_| "Email recipient is missing")?;
    let result=client.post("https://api.resend.com/emails").bearer_auth(key).json(&json!({"from":from,"to":[to],"subject":"Crypto Bros — paper portfolio risk review","text":message})).send().await.map_err(|_|"Email delivery connection failed")?;
    if result.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "Email provider returned HTTP {}",
            result.status().as_u16()
        ))
    }
}
