use sqlx::PgConnection;

#[async_trait::async_trait]
pub trait SecretKeyProvider {
    async fn valid_secret_key(&mut self, secret_key: &str) -> Result<bool, sqlx::Error>;
}

#[async_trait::async_trait]
impl SecretKeyProvider for PgConnection {
    async fn valid_secret_key(&mut self, secret_key: &str) -> Result<bool, sqlx::Error> {
        let secret_keys = sqlx::query!(
            r#"
                SELECT
                    secret_key
                FROM
                    secret_keys
                WHERE
                    secret_key = $1
            "#,
            secret_key,
        )
        .fetch_all(self)
        .await?;

        Ok(!secret_keys.is_empty())
    }
}
