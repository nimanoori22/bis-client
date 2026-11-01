//! Explicit network example; normal tests never contact BIS.
use bis_client::{Client, Query};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(2..=4).contains(&args.len()) {
        return Err(
            "usage: cargo run --example fetch -- <flow> <key> [startPeriod] [endPeriod]".into(),
        );
    }
    let mut query = Query::new(&args[0], &args[1]);
    query.start_period = args.get(2).cloned();
    query.end_period = args.get(3).cloned();
    let result = Client::new()?.fetch(&query).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "request_url": result.request_url,
            "response_url": result.response_url,
            "retrieved_at": result.retrieved_at,
            "status": result.status,
            "sha256": result.sha256,
            "decoded_bytes": result.body.len(),
            "columns": result.data.columns,
            "observations": result.data.observations.len(),
            "first": result.data.observations.first(),
            "last": result.data.observations.last(),
        }))?
    );
    Ok(())
}
