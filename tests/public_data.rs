// Explicit, read-only provider checks. No .env, wallet, session store or API key.
#[tokio::test]
#[ignore = "requires public internet"]
async fn public_solusdc_candles_and_keyless_jupiter_quote() {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("client");
    let (candles, price) = crypto_bros::market::candles(&client)
        .await
        .expect("public candles");
    assert!(candles.len() >= 600);
    assert!(price > 0.0);
    println!(
        "Public SOLUSDC: {} completed candles; market price parsed",
        candles.len()
    );
    let quote = crypto_bros::market::quote(&client, 150_000_000, None)
        .await
        .expect("keyless quote");
    assert!(quote.sol_out > 0.0);
    assert_eq!(quote.amount_usdc, 150.0);
    println!("Jupiter quote-only response parsed; no taker, signing, or submission");
}

#[tokio::test]
#[ignore = "requires public internet"]
async fn public_thb_valuation_and_mainnet_rpc_identity() {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();
    let rate = crypto_bros::chain::rate(&client)
        .await
        .expect("public USDC/THB rate");
    assert!(rate.fresh(chrono::Utc::now().timestamp()));
    let value: serde_json::Value = client
        .post("https://api.mainnet-beta.solana.com")
        .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"getGenesisHash","params":[]}))
        .send()
        .await
        .expect("public RPC")
        .error_for_status()
        .expect("public RPC status")
        .json()
        .await
        .expect("RPC JSON");
    assert_eq!(
        value["result"],
        "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"
    );
    println!(
        "Public USDC/THB valuation and mainnet identity verified; no wallet address or transaction submitted"
    );
}
