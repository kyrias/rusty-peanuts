use serde::Serialize;
use sqlx::{Connection, FromRow, PgConnection};

use crate::models;
use rusty_peanuts_api_structs::Source;

pub type PhotoId = i32;

#[derive(Debug, PartialEq, Serialize)]
pub enum Page {
    Latest,
    Before(u32),
    After(u32),
}

impl Page {
    fn order_direction(&self) -> &'static str {
        match self {
            Page::Latest => "DESC",
            Page::Before(_) => "DESC",
            Page::After(_) => "ASC",
        }
    }
}

impl From<Option<i32>> for Page {
    fn from(page_id: Option<i32>) -> Self {
        match page_id {
            None => Page::Latest,
            Some(photo_id) if photo_id >= 0 => Page::Before(photo_id as u32),
            Some(photo_id) if photo_id < 0 => Page::After((-photo_id - 1) as u32),
            Some(_) => unreachable!("i32 cannot be neither >=0 nor <0 at the same time"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Published {
    All,
    OnlyPublished,
}

#[derive(Debug)]
enum BindValue<'a> {
    U32(u32),
    I64(i64),
    ArrayString(&'a [String]),
}

#[derive(Debug, FromRow)]
pub struct Photo {
    pub id: PhotoId,
    pub file_stem: String,
    pub title: Option<String>,
    pub taken_timestamp: Option<String>,
    pub height_offset: i32,
    pub tags: Vec<String>,
    pub sources: sqlx::types::Json<Vec<Source>>,
    pub published: bool,
}

#[async_trait::async_trait]
pub trait PhotoProvider {
    async fn get_paginated_photos(
        &mut self,
        limit: i64,
        page: Page,
        tagged: &Option<Vec<String>>,
        published: Published,
    ) -> Result<Vec<models::photos::Photo>, sqlx::Error>;

    async fn get_photo_pagination_ids(
        &mut self,
        photos: &[models::photos::Photo],
        tagged: &Option<Vec<String>>,
        published: Published,
    ) -> Result<(Option<i32>, Option<i32>), sqlx::Error>;

    async fn get_photo_by_id(
        &mut self,
        photo_id: PhotoId,
        published: Published,
    ) -> Result<Option<models::photos::Photo>, sqlx::Error>;

    async fn get_photo_by_file_stem(
        &mut self,
        file_stem: &str,
        published: Published,
    ) -> Result<Option<models::photos::Photo>, sqlx::Error>;

    async fn get_photo_tags_with_counts(
        &mut self,
        tagged: &Option<Vec<String>>,
        published: Published,
    ) -> Result<Vec<(String, i64)>, sqlx::Error>;

    async fn insert_photo(&mut self, photo: &models::photos::Photo)
        -> Result<PhotoId, sqlx::Error>;

    async fn update_photo(
        &mut self,
        old_photo: &models::photos::Photo,
        new_photo: &rusty_peanuts_api_structs::PhotoPayload,
    ) -> Result<bool, sqlx::Error>;

    async fn set_photo_published_state(
        &mut self,
        photo_id: PhotoId,
        published: bool,
    ) -> Result<(), sqlx::Error>;

    async fn set_photo_height_offset(
        &mut self,
        photo_id: PhotoId,
        height_offset: u8,
    ) -> Result<(), sqlx::Error>;
}

#[async_trait::async_trait]
impl PhotoProvider for PgConnection {
    async fn get_paginated_photos(
        &mut self,
        limit: i64,
        page: Page,
        tagged: &Option<Vec<String>>,
        published: Published,
    ) -> Result<Vec<models::photos::Photo>, sqlx::Error> {
        let mut bind_count = 1;
        let mut bind_values = Vec::new();
        let mut query = r#"
            SELECT
                id, title, file_stem, taken_timestamp, height_offset, tags, published,
                JSONB_AGG(TO_JSONB(source)) AS "sources"
            FROM
                photos photo
            LEFT JOIN
                sources source
            ON
                source.photo_id = photo.id
        "#
        .to_string();

        tide::log::info!("Page: {:?}", page);
        match page {
            Page::Before(photo_id) => {
                query.push_str(&format!(
                    r#"
                            WHERE
                                id < ${}
                        "#,
                    bind_count,
                ));
                bind_count += 1;
                bind_values.push(BindValue::U32(photo_id));
            },

            Page::After(photo_id) => {
                query.push_str(&format!(
                    r#"
                            WHERE
                                id > ${}
                        "#,
                    bind_count,
                ));
                bind_count += 1;
                bind_values.push(BindValue::U32(photo_id));
            },

            Page::Latest => {
                query.push_str(
                    r#"
                        WHERE
                            true
                    "#,
                );
            },
        }

        if let Some(tags) = tagged {
            query.push_str(&format!(
                r#"
                        AND photo.tags @> ${}::varchar[]
                    "#,
                bind_count,
            ));
            bind_count += 1;
            bind_values.push(BindValue::ArrayString(&tags[..]));
        }

        if published == Published::OnlyPublished {
            query.push_str(
                r#"
                AND photo.published = 't'
            "#,
            );
        }

        query.push_str(&format!(
            r#"
                    GROUP BY
                        id, title, file_stem, taken_timestamp, height_offset, tags, published
                    ORDER BY
                        id {}
                    LIMIT ${}
                "#,
            page.order_direction(),
            bind_count,
        ));
        // Necessary if any more bind variables are added in this function, but leaving it
        // uncommented leads to the complainer complaining, and attributes on expressions are
        // experimental so can't disable the lint without enabling that.
        //bind_count += 1;
        bind_values.push(BindValue::I64(limit));

        let mut query = sqlx::query_as(&query);

        for value in bind_values {
            query = match value {
                BindValue::U32(v) => query.bind(v),
                BindValue::I64(v) => query.bind(v),
                BindValue::ArrayString(v) => query.bind(v),
            };
        }
        let res: Vec<Photo> = query.fetch_all(self).await?;

        let mut photos: Vec<_> = res.into_iter().map(models::photos::Photo::from).collect();
        photos.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(photos)
    }


    async fn get_photo_pagination_ids(
        &mut self,
        photos: &[models::photos::Photo],
        tagged: &Option<Vec<String>>,
        published: Published,
    ) -> Result<(Option<i32>, Option<i32>), sqlx::Error> {
        let previous = match photos.first() {
            Some(photo) => {
                if self
                    .get_paginated_photos(1, Page::After(photo.id as u32), &tagged, published)
                    .await?
                    .is_empty()
                {
                    None
                } else {
                    Some(photo.id)
                }
            },
            None => None,
        };

        let next = match photos.last() {
            Some(photo) => {
                if self
                    .get_paginated_photos(1, Page::Before(photo.id as u32), &tagged, published)
                    .await?
                    .is_empty()
                {
                    None
                } else {
                    Some(photo.id)
                }
            },
            None => None,
        };

        Ok((previous, next))
    }


    async fn get_photo_by_id(
        &mut self,
        photo_id: PhotoId,
        published: Published,
    ) -> Result<Option<models::photos::Photo>, sqlx::Error> {
        let mut query = r#"
            SELECT
                id, title, file_stem, taken_timestamp, height_offset, tags, published,
                JSONB_AGG(TO_JSONB(source)) AS "sources"
            FROM
                photos photo
            LEFT JOIN
                sources source
            ON
                source.photo_id = photo.id
            WHERE
                id = $1
        "#
        .to_string();

        if published == Published::OnlyPublished {
            query.push_str("    AND photo.published = 't'\n")
        }

        query.push_str(
            r#"
            GROUP BY
                id, title, file_stem
            ORDER BY id DESC
        "#,
        );

        let res: Result<Photo, _> = sqlx::query_as(&query).bind(photo_id).fetch_one(self).await;

        match res {
            Ok(photo) => Ok(Some(photo.into())),
            Err(sqlx::Error::RowNotFound) => Ok(None),
            Err(err) => Err(err),
        }
    }


    async fn get_photo_by_file_stem(
        &mut self,
        file_stem: &str,
        published: Published,
    ) -> Result<Option<models::photos::Photo>, sqlx::Error> {
        let mut query = r#"
            SELECT
                id, title, file_stem, taken_timestamp, height_offset, tags, published,
                JSONB_AGG(TO_JSONB(source)) AS "sources"
            FROM
                photos photo
            LEFT JOIN
                sources source
            ON
                source.photo_id = photo.id
            WHERE
                file_stem = $1
        "#
        .to_string();

        if published == Published::OnlyPublished {
            query.push_str("    AND photo.published = 't'\n")
        }

        query.push_str(
            r#"
            GROUP BY
                id, title, file_stem
            ORDER BY id DESC
        "#,
        );

        let res: Result<Photo, _> = sqlx::query_as(&query).bind(file_stem).fetch_one(self).await;

        match res {
            Ok(photo) => Ok(Some(photo.into())),
            Err(sqlx::Error::RowNotFound) => Ok(None),
            Err(err) => Err(err),
        }
    }


    async fn get_photo_tags_with_counts(
        &mut self,
        tagged: &Option<Vec<String>>,
        published: Published,
    ) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let bind_count = 1;
        let mut bind_values = Vec::new();

        let mut query = r#"
            SELECT DISTINCT
                UNNEST(tags) AS tag, COUNT(*) AS count
            FROM
                photos photo
            WHERE
                true
        "#
        .to_string();

        if let Some(tags) = tagged {
            query.push_str(&format!(
                r#"
                        AND photo.tags @> ${}::varchar[]
                    "#,
                bind_count,
            ));
            // Necessary if any more bind variables are added in this function, but leaving it
            // uncommented leads to the complainer complaining, and attributes on expressions are
            // experimental so can't disable the lint without enabling that.
            //bind_count += 1;
            bind_values.push(BindValue::ArrayString(tags));
        }

        if published == Published::OnlyPublished {
            query.push_str("    AND photo.published = 't'\n")
        }

        query.push_str(
            r#"
            GROUP BY
                tag
            ORDER BY
                tag
        "#,
        );

        let mut query = sqlx::query_as(&query);

        for value in bind_values {
            query = match value {
                BindValue::U32(v) => query.bind(v),
                BindValue::I64(v) => query.bind(v),
                BindValue::ArrayString(v) => query.bind(v),
            };
        }

        let tags_with_counts: Vec<(String, i64)> = query.fetch_all(self).await?;

        Ok(tags_with_counts)
    }


    async fn insert_photo(
        &mut self,
        photo: &models::photos::Photo,
    ) -> Result<PhotoId, sqlx::Error> {
        let mut trans = self.begin().await?;

        let res = sqlx::query!(
            r#"
                INSERT INTO photos
                    (title, file_stem, taken_timestamp, height_offset, tags, published)
                VALUES
                    ($1, $2, $3, $4, $5, $6)
                RETURNING
                    id
            "#,
            photo.title,
            photo.file_stem,
            photo.taken_timestamp,
            photo.height_offset as i32,
            &photo.tags,
            photo.published,
        )
        .fetch_one(&mut trans)
        .await?;

        for source in &photo.sources {
            sqlx::query!(
                r#"
                    INSERT INTO sources
                        (photo_id, width, height, url)
                    VALUES
                        ($1, $2, $3, $4)
                "#,
                res.id,
                source.width as i32,
                source.height as i32,
                source.url,
            )
            .execute(&mut trans)
            .await?;
        }

        trans.commit().await?;

        Ok(res.id)
    }


    async fn update_photo(
        &mut self,
        old_photo: &models::photos::Photo,
        new_photo: &rusty_peanuts_api_structs::PhotoPayload,
    ) -> Result<bool, sqlx::Error> {
        let mut trans = self.begin().await?;
        let mut changed = false;

        if old_photo.taken_timestamp != new_photo.taken_timestamp {
            tide::log::info!("Taken timestamp differs, updating");
            changed = true;
            sqlx::query!(
                r#"
                    UPDATE
                        photos
                    SET
                        taken_timestamp = $2
                    WHERE
                        id = $1
                "#,
                old_photo.id,
                new_photo.taken_timestamp,
            )
            .execute(&mut trans)
            .await?;
        }

        if old_photo.title != new_photo.title {
            tide::log::info!("Title differs, updating");
            changed = true;
            sqlx::query!(
                r#"
                    UPDATE
                        photos
                    SET
                        title = $2
                    WHERE
                        id = $1
                "#,
                old_photo.id,
                new_photo.title,
            )
            .execute(&mut trans)
            .await?;
        }

        if old_photo.tags != new_photo.tags {
            tide::log::info!("Tags differ, updating");
            changed = true;
            sqlx::query!(
                r#"
                    UPDATE
                        photos
                    SET
                        tags = $2
                    WHERE
                        id = $1
                "#,
                old_photo.id,
                &new_photo.tags,
            )
            .execute(&mut trans)
            .await?;
        }

        if let Some(sources) = &new_photo.sources {
            if &old_photo.sources != sources {
                tide::log::info!("Sources differ, updating");
                changed = true;
                sqlx::query!(
                    r#"
                        DELETE FROM
                            sources
                        WHERE
                            photo_id = $1
                    "#,
                    old_photo.id,
                )
                .execute(&mut trans)
                .await?;

                for source in sources {
                    sqlx::query!(
                        r#"
                            INSERT INTO sources
                                (photo_id, width, height, url)
                            VALUES
                                ($1, $2, $3, $4)
                        "#,
                        old_photo.id,
                        source.width as i32,
                        source.height as i32,
                        source.url,
                    )
                    .execute(&mut trans)
                    .await?;
                }
            }
        }

        trans.commit().await?;
        Ok(changed)
    }


    async fn set_photo_published_state(
        &mut self,
        photo_id: PhotoId,
        published: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
                UPDATE
                    photos
                SET
                    published = $1
                WHERE
                    photos.id = $2
            "#,
            published,
            photo_id,
        )
        .execute(self)
        .await?;

        Ok(())
    }


    async fn set_photo_height_offset(
        &mut self,
        photo_id: PhotoId,
        height_offset: u8,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
                UPDATE
                    photos
                SET
                    height_offset = $1
                WHERE
                    photos.id = $2
            "#,
            height_offset as i32,
            photo_id,
        )
        .execute(self)
        .await?;

        Ok(())
    }
}
