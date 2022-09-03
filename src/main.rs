use anyhow::Result;

#[async_std::main]
async fn main() -> Result<()> {
    rusty_peanuts::main().await?;

    Ok(())
}
