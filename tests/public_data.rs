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
