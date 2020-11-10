use sqlx::PgConnection;
use tide::Request;

use crate::db::secret_keys::SecretKeyProvider;

pub async fn validate_secret_key(
    req: &Request<crate::State>,
    conn: &mut PgConnection,
) -> Result<Option<bool>, sqlx::Error> {
    let auth = match req.header("Authorization") {
        Some(value) => value,
        None => return Ok(None),
    };

    let parts: Vec<_> = auth.last().as_str().splitn(2, ' ').collect();

    if parts[0] == "Bearer" && conn.valid_secret_key(parts[1]).await? {
        return Ok(Some(true));
    }

    Ok(Some(false))
}

macro_rules! require_valid_secret_key {
    ($request:ident, $connection:ident) => {
        use tide::Response;
        match validate_secret_key(&$request, &mut $connection).await? {
            None => return Ok(Response::builder(tide::http::StatusCode::Unauthorized).build()),
            Some(false) => return Ok(Response::builder(tide::http::StatusCode::Forbidden).build()),
            Some(true) => {},
        }
    };
}
