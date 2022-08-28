use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use tide::{Request, Response};

use crate::db::photos::{PhotoProvider, Published};
use crate::db::secret_keys::SecretKeyProvider;

pub(in super::super) fn mount(route: &mut tide::Server<crate::State>) {
    route.at("/").get(gallery);
    route.at("/sitemap.xml").get(sitemap);

    route.at("/tagged/:tagged").get(gallery);

    route.at("/photo/:photo_id").get(single_photo);
    route
        .at("/photo/:photo_id/multi")
        .get(single_photo_multiple_times);
}

async fn allowed_publish_status(
    req: &Request<crate::State>,
    conn: &mut PgConnection,
) -> Result<Published, sqlx::Error> {
    let published = match req.cookie("secret-key") {
        Some(secret_key) => {
            tide::log::info!("secret key found");
            if conn.valid_secret_key(secret_key.value()).await? {
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
            percent_encoding::percent_decode_str(tag)
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
        .get_photo_page(limit.into(), query.offset.into(), &tagged, published)
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
    context.insert("cache_buster", &state.cache_busting_string);
    match tagged {
        Some(tag) => {
            context.insert("title", &format!("tagged {}", tag[0]));
            let canonical_href = if let Some(offset) = query.offset {
                format!(
                    "{}/tagged/{}?offset={}",
                    state.args.base_url, tag[0], offset
                )
            } else {
                format!("{}/tagged/{}", state.args.base_url, tag[0])
            };
            context.insert("canonical_href", &canonical_href);
        },
        None => {
            context.insert("title", "gallery");
            let canonical_href = if let Some(offset) = query.offset {
                format!("{}/?offset={}", state.args.base_url, offset)
            } else {
                format!("{}/", state.args.base_url)
            };
            context.insert("canonical_href", &canonical_href);
        },
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

async fn sitemap(req: Request<crate::State>) -> tide::Result<Response> {
    let state = req.state();
    let mut conn = state.db.acquire().await?;

    let published = allowed_publish_status(&req, &mut conn).await?;

    let mut buf = Vec::new();
    let sitemap_writer = sitemap::writer::SiteMapWriter::new(&mut buf);
    let mut urlwriter = sitemap_writer.start_urlset()?;

    urlwriter.url(format!("{}/", state.args.base_url))?;

    for (tag, _) in conn.get_photo_tags_with_counts(&None, published).await? {
        urlwriter.url(format!("{}/tagged/{}", state.args.base_url, tag))?;
    }

    for id in conn.get_all_photo_ids(published).await? {
        urlwriter.url(format!("{}/photo/{}", state.args.base_url, id))?;
    }

    urlwriter.end()?;

    let res = Response::builder(tide::http::StatusCode::Ok)
        .body(buf)
        .content_type(tide::http::mime::XML)
        .build();
    Ok(res)
}

async fn photo_internal(
    req: Request<crate::State>,
    mut context: tera::Context,
    template: &str,
) -> tide::Result<Response> {
    let state = req.state();
    let mut conn = state.db.acquire().await?;

    let photo_id = req.param("photo_id")?.parse::<i32>()?;

    let published = allowed_publish_status(&req, &mut conn).await?;
    let res = conn.get_photo_by_id(photo_id, published).await?;

    let photo = match res {
        Some((photo, newer, older)) => {
            if let Some(newer_id) = newer {
                context.insert("newer_id", &newer_id);
            }
            if let Some(older_id) = older {
                context.insert("older_id", &older_id);
            }
            photo
        },
        None => return Ok(Response::builder(tide::http::StatusCode::NotFound).build()),
    };

    context.insert("cache_buster", &state.cache_busting_string);
    match photo.title {
        Some(ref title) => context.insert("title", &title),
        None => context.insert("title", "Untitled"),
    }
    context.insert("photo", &photo);

    let body = state.tera.render(template, &context)?;
    let res = Response::builder(tide::http::StatusCode::Ok)
        .content_type("text/html")
        .body(body)
        .build();
    Ok(res)
}

async fn single_photo(req: Request<crate::State>) -> tide::Result<Response> {
    let state = req.state();
    let mut context = tera::Context::new();

    let photo_id = req.param("photo_id")?;
    context.insert(
        "canonical_href",
        &format!("{}/photo/{}", state.args.base_url, photo_id),
    );

    photo_internal(req, context, "photo.html").await
}

async fn single_photo_multiple_times(req: Request<crate::State>) -> tide::Result<Response> {
    let state = req.state();
    let mut context = tera::Context::new();

    let photo_id = req.param("photo_id")?;
    context.insert(
        "canonical_href",
        &format!("{}/photo/{}/multi", state.args.base_url, photo_id),
    );

    photo_internal(req, context, "single-photo-multiple-times.html").await
}
