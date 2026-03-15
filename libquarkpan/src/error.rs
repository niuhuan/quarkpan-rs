use thiserror::Error;

pub type Result<T> = std::result::Result<T, QuarkPanError>;

#[derive(Debug, Error)]
pub enum QuarkPanError {
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("operation cancelled")]
    Cancelled,

    #[error("remote api error: status={status}, message={message}")]
    Api { status: u32, message: String },

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("http middleware error: {0}")]
    HttpMiddleware(#[from] reqwest_middleware::Error),

    #[error("json error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("header value error: {0}")]
    HeaderValue(#[from] reqwest::header::InvalidHeaderValue),

    #[error("url parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

impl QuarkPanError {
    pub fn missing_field(field: &'static str) -> Self {
        Self::MissingField(field)
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::InvalidArgument(message.into())
    }
}
