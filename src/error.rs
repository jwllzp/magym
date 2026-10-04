use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("http request failed: {0}")]
    Http(#[from] Box<ureq::Error>),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid mana cost: {0:?}")]
    ManaCost(String),
    #[error("invalid type line: {0:?}")]
    TypeLine(String),
    #[error("invalid color: {0:?}")]
    Color(String),
    #[error("invalid rarity: {0:?}")]
    Rarity(String),
    #[error("missing field: {0}")]
    MissingField(&'static str),
    #[error("unsupported card layout: {0}")]
    Unsupported(String),
    #[error("invalid set code: {0:?}")]
    SetCode(String),
    #[error("no cards found for set {0:?}")]
    UnknownSet(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml error: {0}")]
    TomlWrite(#[from] toml::ser::Error),
    #[error("{}: {source}", path.display())]
    CardFile {
        path: std::path::PathBuf,
        source: toml::de::Error,
    },
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        Error::Http(Box::new(e))
    }
}
