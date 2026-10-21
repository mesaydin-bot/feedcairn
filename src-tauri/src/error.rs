use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Xml(#[from] roxmltree::Error),
    #[error("feed parse error: {0}")]
    FeedParse(String),
    #[error("{hint}")]
    CliMissing {
        provider: String,
        command: String,
        hint: String,
    },
    #[error("AI command timed out after {0}s")]
    AiTimeout(u64),
    #[error("AI command failed: {0}")]
    AiFailed(String),
}

impl Error {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("error", &crate::models::mask_home(&self.to_string()))?;
        if let Error::CliMissing {
            provider,
            command,
            hint,
        } = self
        {
            map.serialize_entry("code", "cli_missing")?;
            map.serialize_entry("provider", provider)?;
            map.serialize_entry("command", &crate::models::display_command(command))?;
            map.serialize_entry("hint", &crate::models::mask_home(hint))?;
        } else {
            map.serialize_entry("code", "error")?;
        }
        map.end()
    }
}

impl From<feed_rs::parser::ParseFeedError> for Error {
    fn from(value: feed_rs::parser::ParseFeedError) -> Self {
        Error::FeedParse(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
