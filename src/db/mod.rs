use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

pub mod photos;
pub mod secret_keys;

pub async fn get_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .min_connections(1)
        .max_connections((num_cpus::get_physical() * 2) as u32)
        .connect_timeout(Duration::from_secs(2))
        .connect(database_url)
        .await
}
