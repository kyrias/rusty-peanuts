use std::io::Read;
use std::sync::Arc;

use anyhow::{Context, Result};
use opentelemetry_tide::TideExt;
use structopt::StructOpt;

pub mod db;
pub mod models;
pub mod telemetry;
pub mod web;

#[derive(Clone, Debug)]
pub struct State {
    pub args: Arc<Args>,
    pub db: sqlx::postgres::PgPool,
    pub tera: Arc<tera::Tera>,
    pub cache_busting_string: Option<String>,
}

#[derive(Debug, StructOpt)]
pub struct Args {
    /// Host address to bind to.
    #[structopt(long, default_value = "localhost", env = "RUSTY_PEANUTS_BIND_ADDRESS")]
    address: String,
    /// Port to bind to.
    #[structopt(long, default_value = "8166", env = "RUSTY_PEANUTS_BIND_PORT")]
    port: u16,

    /// PostgreSQL database url.
    #[structopt(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,

    /// Gallery base URL.
    #[structopt(long, env = "RUSTY_PEANUTS_BASE_URL")]
    base_url: String,

    /// Default number of photos per gallery page
    #[structopt(
        long,
        default_value = "10",
        env = "RUSTY_PEANUTS_DEFAULT_PHOTOS_PER_PAGE"
    )]
    default_photos_per_page: u8,

    /// Max number of photos per gallery page
    #[structopt(long, default_value = "100", env = "RUSTY_PEANUTS_MAX_PHOTOS_PER_PAGE")]
    max_photos_per_page: u8,

    /// Path to Tera templates directory
    #[structopt(
        long,
        parse(from_os_str),
        default_value = "./templates",
        env = "RUSTY_PEANUTS_TEMPLATE_PATH"
    )]
    template_path: std::path::PathBuf,
}

pub async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = Arc::new(Args::from_args());

    telemetry::init().context("Failed to initialize telemetry module")?;

    let pool = db::get_pool(&args.database_url)
        .await
        .context("Failed to get database pool")?;

    let template_path = args
        .template_path
        .canonicalize()
        .context("Could not canonicalize template path")?;
    let tera = tera::Tera::new(&template_path.join("**/*.html").to_string_lossy())
        .context("Failed to parse templates")?;

    let cache_busting_string = match std::fs::File::open(template_path.join("cache-buster")) {
        Ok(mut file) => {
            let mut data = String::new();
            file.read_to_string(&mut data)
                .context("Couldn't read cache-busting string")?;
            data.split_whitespace().next().map(|s| s.to_string())
        },
        Err(_) => None,
    };

    let state = State {
        args: args.clone(),
        db: pool,
        tera: Arc::new(tera),
        cache_busting_string,
    };
    let mut app = tide::with_state(state);

    app.with_default_tracing_middleware();

    web::mount(&mut app);

    let address: &str = args.address.as_ref();
    app.listen((address, args.port))
        .await
        .context("Failed to start tide app")?;

    Ok(())
}
