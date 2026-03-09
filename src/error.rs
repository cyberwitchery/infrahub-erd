//! error types
//!
//! structured errors for config, io, infrahub client, and schema parsing.

/// library result type
pub type Result<T> = std::result::Result<T, Error>;

/// error type for infrahub-erd
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("infrahub error: {0}")]
    Infrahub(#[from] infrahub::Error),

    #[error("schema parse error: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_error_display() {
        let err = Error::Config("missing url".to_string());
        assert_eq!(err.to_string(), "config error: missing url");
    }

    #[test]
    fn test_parse_error_display() {
        let err = Error::Parse("unexpected token".to_string());
        assert_eq!(err.to_string(), "schema parse error: unexpected token");
    }

    #[test]
    fn test_io_error_from() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }
}
