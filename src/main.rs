#[async_std::main]
async fn main() -> Result<(), rusty_peanuts::Error> {
    rusty_peanuts::main().await?;

    Ok(())
}
