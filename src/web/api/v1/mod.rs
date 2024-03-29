use tide::{Request, Response};
use tracing::{info, instrument};

use crate::db::photos::{PhotoProvider, Published};
use crate::web::api::utils::validate_secret_key;
use rusty_peanuts_api_structs::PhotoPayload;

pub(super) fn mount(mut route: tide::Route<crate::State>) {
    route.at("/photos").post(create_photo);

    route.at("/photo/by-id/:photo_id").get(get_photo);
    route
        .at("/photo/by-id/:photo_id/published")
        .post(update_photo_published);
    route
        .at("/photo/by-id/:photo_id/height-offset")
        .post(update_photo_height_offset);

    route
        .at("/photo/by-filestem/:file_stem")
        .get(get_photo_by_file_stem)
        .post(update_photo);
}

#[instrument(skip_all)]
async fn get_photo(req: Request<crate::State>) -> tide::Result<Response> {
    let state = req.state();
    let mut conn = state
        .db
        .acquire()
        .await
        .expect("couldn't get DB connection");

    let published = match validate_secret_key(&req, &mut conn).await? {
        None => Published::OnlyPublished,
        Some(false) => Published::OnlyPublished,
        Some(true) => Published::All,
    };

    let photo_id: i32 = req.param("photo_id")?.parse()?;
    let res = match conn.get_photo_by_id(photo_id, published).await? {
        Some((photo, _, _)) => Response::builder(tide::http::StatusCode::Ok)
            .body(tide::Body::from_json(&photo)?)
            .build(),
        None => Response::builder(tide::http::StatusCode::NotFound).build(),
    };

    Ok(res)
}

#[instrument(skip_all)]
async fn create_photo(mut req: Request<crate::State>) -> tide::Result<Response> {
    let state = req.state();
    let mut conn = state
        .db
        .acquire()
        .await
        .expect("couldn't get DB connection");

    require_valid_secret_key!(req, conn);

    let payload: PhotoPayload = req.body_json().await?;
    info!(payload = ?payload, "Received valid payload");

    let sources = match payload.sources {
        Some(sources) => sources,
        None => {
            return Ok(Response::builder(tide::http::StatusCode::BadRequest).build());
        },
    };

    let new_photo = crate::models::photos::Photo {
        file_stem: payload.file_stem.clone(),
        title: payload.title,
        taken_timestamp: payload.taken_timestamp,
        tags: payload.tags,
        sources,
        published: false,
        ..Default::default()
    };

    let old_photo = conn
        .get_photo_by_file_stem(&payload.file_stem, Published::All)
        .await?;
    match old_photo {
        Some(photo) => Ok(Response::builder(tide::http::StatusCode::Conflict)
            .body(tide::convert::json!({
                "reason": format!("Photo with file stem {} already exists.", &payload.file_stem),
                "existing": photo,
            }))
            .build()),
        None => {
            let id = conn.insert_photo(&new_photo).await?;
            let created_photo = conn
                .get_photo_by_id(id, Published::All)
                .await?
                .map(|(photo, _, _)| photo);

            Ok(Response::builder(tide::http::StatusCode::Created)
                .body(tide::convert::json!({
                    "id": id,
                    "created": created_photo,
                }))
                .build())
        },
    }
}

#[instrument(skip_all)]
async fn get_photo_by_file_stem(req: Request<crate::State>) -> tide::Result<Response> {
    let state = req.state();
    let mut conn = state
        .db
        .acquire()
        .await
        .expect("couldn't get DB connection");

    let published = match validate_secret_key(&req, &mut conn).await? {
        None => Published::OnlyPublished,
        Some(false) => Published::OnlyPublished,
        Some(true) => Published::All,
    };

    let file_stem = req.param("file_stem")?;
    let res = match conn.get_photo_by_file_stem(file_stem, published).await? {
        Some(photo) => Response::builder(tide::http::StatusCode::Ok)
            .body(tide::Body::from_json(&photo)?)
            .build(),
        None => Response::builder(tide::http::StatusCode::NotFound).build(),
    };

    Ok(res)
}

#[instrument(skip_all)]
async fn update_photo(mut req: Request<crate::State>) -> tide::Result<Response> {
    let state = req.state();
    let mut conn = state
        .db
        .acquire()
        .await
        .expect("couldn't get DB connection");

    require_valid_secret_key!(req, conn);

    let payload: PhotoPayload = req.body_json().await?;
    info!(payload = ?payload, "Received valid payload");

    let file_stem = req.param("file_stem")?;
    let old_photo = match conn
        .get_photo_by_file_stem(file_stem, Published::All)
        .await?
    {
        Some(photo) => photo,
        None => return Ok(Response::builder(tide::http::StatusCode::NotFound).build()),
    };

    let changed = conn.update_photo(&old_photo, &payload).await?;
    let updated_photo = conn
        .get_photo_by_id(old_photo.id, Published::All)
        .await?
        .map(|(photo, _, _)| photo);

    Ok(Response::builder(tide::http::StatusCode::Ok)
        .body(tide::convert::json!({
            "changed": changed,
            "previous": old_photo,
            "current": updated_photo,
        }))
        .build())
}

#[instrument(skip_all)]
async fn update_photo_published(mut req: Request<crate::State>) -> tide::Result<Response> {
    let state = req.state();
    let mut conn = state
        .db
        .acquire()
        .await
        .expect("couldn't get DB connection");

    require_valid_secret_key!(req, conn);

    let published: bool = req.body_json().await?;

    let photo_id: i32 = req.param("photo_id")?.parse()?;
    let photo = match conn.get_photo_by_id(photo_id, Published::All).await? {
        Some((photo, _, _)) => photo,
        None => return Ok(Response::builder(tide::http::StatusCode::NotFound).build()),
    };

    conn.set_photo_published_state(photo.id, published).await?;

    Ok(Response::builder(tide::http::StatusCode::Ok)
        .body(tide::convert::json!({
            "published": published,
        }))
        .build())
}

#[instrument(skip_all)]
async fn update_photo_height_offset(mut req: Request<crate::State>) -> tide::Result<Response> {
    let state = req.state();
    let mut conn = state
        .db
        .acquire()
        .await
        .expect("couldn't get DB connection");

    require_valid_secret_key!(req, conn);

    let height_offset: u8 = req.body_json().await?;

    let photo_id: i32 = req.param("photo_id")?.parse()?;
    let photo = match conn.get_photo_by_id(photo_id, Published::All).await? {
        Some((photo, _, _)) => photo,
        None => return Ok(Response::builder(tide::http::StatusCode::NotFound).build()),
    };

    conn.set_photo_height_offset(photo.id, height_offset)
        .await?;

    Ok(Response::builder(tide::http::StatusCode::NoContent).build())
}
