fn main() -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&email_server::api::openapi_value())?
    );
    Ok(())
}
