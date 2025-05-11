use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AvanteCurlError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
    
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),
    
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("Request was cancelled")]
    Cancelled,
    
    #[error("Request timed out")]
    Timeout,
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Session error: {0}")]
    SessionError(String),
    
    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for AvanteCurlError {
    fn from(err: anyhow::Error) -> Self {
        AvanteCurlError::Other(err.to_string())
    }
}
