#[derive(Debug)]
pub enum PlinkoError {
    Generic(String),
    InvalidWorkload,
}

impl std::fmt::Display for PlinkoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlinkoError::Generic(s) => write!(f, "{}", s),
            PlinkoError::InvalidWorkload => write!(f, "invalid workload"),
        }
    }
}

impl std::error::Error for PlinkoError {}

impl From<String> for PlinkoError {
    fn from(value: String) -> Self {
        PlinkoError::Generic(value)
    }
}

impl From<&str> for PlinkoError {
    fn from(value: &str) -> Self {
        PlinkoError::Generic(value.to_string())
    }
}

pub type PlinkoResult<T> = Result<T, PlinkoError>;
