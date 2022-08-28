use serde::{Deserialize, Serialize};

use rusty_peanuts_api_structs::Source;

pub type PhotoId = i32;

#[derive(Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Photo {
    pub id: PhotoId,
    pub file_stem: String,
    pub title: Option<String>,
    pub taken_timestamp: Option<String>,
    pub height_offset: u8,
    pub tags: Vec<String>,
    pub sources: Vec<Source>,
    pub published: bool,
}

impl From<crate::db::photos::Photo> for Photo {
    fn from(mut p: crate::db::photos::Photo) -> Self {
        p.sources.sort_by(|a, b| b.width.cmp(&a.width));

        Photo {
            id: p.id,
            file_stem: p.file_stem,
            title: p.title,
            taken_timestamp: p.taken_timestamp,
            height_offset: p.height_offset as u8,
            tags: p.tags,
            sources: p.sources.to_vec(),
            published: p.published,
        }
    }
}
