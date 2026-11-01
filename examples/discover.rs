//! Explicit, bounded live smoke check of discovery and CSV detail modes.
use bis_client::{AvailabilityMode, Client, DataDetail, Query, StructureRef};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let flows = client.dataflows().await?;
    let flow = flows
        .iter()
        .find(|flow| flow.id == "WS_EER")
        .ok_or("BIS returned no exchange-rate dataflow")?;
    let structures = client
        .datastructure(&StructureRef::from_urn(&flow.structure)?)
        .await?;
    let structure = structures
        .first()
        .ok_or("BIS returned no exchange-rate structure")?;
    let dimensions = structure.dimensions();
    let enumeration = dimensions
        .first()
        .and_then(|d| d.local_representation.as_ref())
        .and_then(|r| r.enumeration.as_ref())
        .ok_or("frequency codelist reference missing")?;
    let lists = client
        .codelist(&StructureRef::from_urn(enumeration)?)
        .await?;
    let mut query = Query::new("BIS,WS_EER,1.0", "M.N.B.CH");
    query.start_period = Some("2024-01".into());
    query.end_period = Some("2024-02".into());
    let availability = client
        .availability(&query, "all", AvailabilityMode::Exact)
        .await?;
    let full = client.fetch(&query).await?;
    if full.data.observations.len() != 2 {
        return Err("expected two monthly observations".into());
    }
    for detail in [
        DataDetail::DataOnly,
        DataDetail::SeriesKeysOnly,
        DataDetail::NoData,
    ] {
        let table = client.fetch_table(&query, detail).await?;
        println!("{detail:?}: {} rows", table.data.rows.len());
    }
    println!(
        "{}: {} dimensions, {} codelists, {} constraints, {} observations",
        flow.id,
        dimensions.len(),
        lists.len(),
        availability.len(),
        full.data.observations.len()
    );
    Ok(())
}
