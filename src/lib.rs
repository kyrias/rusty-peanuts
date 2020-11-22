use std::sync::Arc;

use structopt::StructOpt;

pub mod db;
pub mod models;
pub mod web;

#[derive(Clone, Debug)]
pub struct State {
    pub args: Arc<Args>,
    pub db: sqlx::postgres::PgPool,
    pub tera: Arc<tera::Tera>,
}

#[derive(Debug)]
pub enum Error {
    TemplateParseError(tera::Error),
}

impl From<Error> for i32 {
    fn from(error: Error) -> i32 {
        match error {
            Error::TemplateParseError(_) => 3,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::TemplateParseError(err) => {
                write!(f, "Template parsing error: {}", err)
            },
        }
    }
}

#[derive(Debug, StructOpt)]
pub struct Args {
    /// Log level.
    #[structopt(long, default_value = "INFO", env = "RUSTY_PEANUTS_LOG_LEVEL")]
    log_level: tide::log::LevelFilter,

    /// Host address to bind to.
    #[structopt(long, default_value = "localhost", env = "RUSTY_PEANUTS_BIND_ADDRESS")]
    address: String,
    /// Port to bind to.
    #[structopt(long, default_value = "8166", env = "RUSTY_PEANUTS_BIND_PORT")]
    port: u16,

    /// PostgreSQL database url.
    #[structopt(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,

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

pub async fn main() -> Result<(), Error> {
    dotenv::dotenv().ok();
    let args = Arc::new(Args::from_args());

    tide::log::with_level(args.log_level);

    let pool = db::get_pool(&args.database_url)
        .await
        .expect("couldn't get DB pool");

    let template_path = args
        .template_path
        .canonicalize()
        .expect("could not canonicalize template path")
        .join("**/*.html");
    let tera = match tera::Tera::new(&template_path.to_string_lossy()) {
        Ok(t) => t,
        Err(e) => {
            return Err(Error::TemplateParseError(e));
        },
    };

    let state = State {
        args: args.clone(),
        db: pool,
        tera: Arc::new(tera),
    };
    let mut app = tide::with_state(state);

    web::mount(&mut app);

    app.listen((args.address.as_ref(), args.port))
        .await
        .expect("starting tide app failed");

    Ok(())
}
