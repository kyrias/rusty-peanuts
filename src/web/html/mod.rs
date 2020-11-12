use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use tide::{Request, Response};

use crate::db::photos::{PhotoProvider, Published};
use crate::db::secret_keys::SecretKeyProvider;

pub(in super::super) fn mount(route: &mut tide::Server<crate::State>) {
    route.at("/").get(gallery);

    route.at("/tagged/:tagged").get(gallery);

    route.at("/photo/:photo_id").get(photo);
    route.at("/photo/:photo_id/multi").get(photo_multiple_times);
}

async fn allowed_publish_status(
    req: &Request<crate::State>,
    conn: &mut PgConnection,
) -> Result<Published, sqlx::Error> {
    let published = match req.cookie("secret-key") {
        Some(secret_key) => {
            tide::log::info!("secret key found");
            if conn.valid_secret_key(&secret_key.value()).await? {
                tide::log::info!("valid");
                Published::All
            } else {
                tide::log::info!("invalid");
                Published::OnlyPublished
            }
        },
        None => Published::OnlyPublished,
    };

    Ok(published)
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct GalleryQueryParams {
    limit: Option<u8>,
    offset: Option<i32>,
}

async fn gallery(req: Request<crate::State>) -> tide::Result<Response> {
    let state = req.state();
    let mut conn = state.db.acquire().await?;

    let published = allowed_publish_status(&req, &mut conn).await?;

    let tagged = req
        .param("tagged")
        .map(|tag| {
            percent_encoding::percent_decode_str(&tag)
                .decode_utf8_lossy()
                .to_string()
        })
        .map(|tag| vec![tag])
        .ok();
    let query: GalleryQueryParams = req.query()?;

    let limit = match query.limit {
        Some(n) if n < state.args.max_photos_per_page => n,
        Some(_) => state.args.default_photos_per_page,
        None => state.args.default_photos_per_page,
    };

    let photos = conn
        .get_paginated_photos(limit.into(), query.offset.into(), &tagged, published)
        .await?;

    let (newer, older) = conn
        .get_photo_pagination_ids(&photos, &tagged, published)
        .await?;

    let tags = conn.get_photo_tags_with_counts(&tagged, published).await?;

    let newest_qs = serde_qs::to_string(&GalleryQueryParams {
        limit: query.limit,
        offset: None,
    })
    .expect("could not encode newest pagination query string");

    let newer_qs = newer.map(|newer_id| {
        serde_qs::to_string(&GalleryQueryParams {
            limit: query.limit,
            offset: Some(-newer_id - 1),
        })
        .expect("could not encode newer pagination query string")
    });

    let older_qs = older.map(|older_id| {
        serde_qs::to_string(&GalleryQueryParams {
            limit: query.limit,
            offset: Some(older_id),
        })
        .expect("could not encode older pagination query string")
    });

    let oldest_qs = serde_qs::to_string(&GalleryQueryParams {
        limit: query.limit,
        offset: Some(-1),
    })
    .expect("could not encode newest pagination query string");

    let mut context = tera::Context::new();
    match tagged {
        Some(tag) => context.insert("title", &format!("tagged {}", tag[0])),
        None => context.insert("title", "gallery"),
    }
    context.insert("photos", &photos);
    context.insert("newest_qs", &newest_qs);
    context.insert("newer_qs", &newer_qs);
    context.insert("older_qs", &older_qs);
    context.insert("oldest_qs", &oldest_qs);
    context.insert("tags", &tags);

    let body = state.tera.render("gallery.html", &context)?;
    let res = Response::builder(tide::http::StatusCode::Ok)
        .content_type("text/html")
        .body(body)
        .build();
    Ok(res)
}

async fn photo(req: Request<crate::State>) -> tide::Result<Response> {
    let state = req.state();
    let mut conn = state.db.acquire().await?;

    let photo_id = req.param("photo_id")?.parse::<i32>()?;

    let published = allowed_publish_status(&req, &mut conn).await?;
    let photo = conn.get_photo_by_id(photo_id, published).await?;

    let photo = match photo {
        Some(photo) => photo,
        None => return Ok(Response::builder(tide::http::StatusCode::NotFound).build()),
    };

    let mut context = tera::Context::new();
    context.insert("title", &format!("photo #{}", photo_id));
    context.insert("photo", &photo);

    let body = state.tera.render("photo.html", &context)?;
    let res = Response::builder(tide::http::StatusCode::Ok)
        .content_type("text/html")
        .body(body)
        .build();
    Ok(res)
}

async fn photo_multiple_times(req: Request<crate::State>) -> tide::Result<Response> {
    let state = req.state();
    let mut conn = state.db.acquire().await?;

    let photo_id = req.param("photo_id")?.parse::<i32>()?;

    let published = allowed_publish_status(&req, &mut conn).await?;
    let photo = conn.get_photo_by_id(photo_id, published).await?;

    let photo = match photo {
        Some(photo) => photo,
        None => return Ok(Response::builder(tide::http::StatusCode::NotFound).build()),
    };

    let mut context = tera::Context::new();
    context.insert("title", &format!("photo #{}", photo_id));
    context.insert("photo", &photo);

    let body = state.tera.render("single-photo-multiple-times.html", &context)?;
    let res = Response::builder(tide::http::StatusCode::Ok)
        .content_type("text/html")
        .body(body)
        .build();
    Ok(res)
}
