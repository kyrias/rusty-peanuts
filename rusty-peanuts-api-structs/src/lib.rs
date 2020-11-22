#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Source {
    pub width: u32,
    pub height: u32,
    pub url: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PhotoPayload {
    pub file_stem: String,
    pub title: Option<String>,
    pub taken_timestamp: Option<String>,
    pub tags: Vec<String>,
    pub sources: Option<Vec<Source>>,
}
