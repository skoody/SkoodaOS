use thiserror::Error;
use std::path::PathBuf;

#[derive(Error, Debug)]
pub enum SkoodaError {
    #[error("IO-Fehler bei {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Mount-Fehler: {source} (Quelle: {source_path:?}, Ziel: {target_path:?})")]
    Mount {
        source_path: Option<PathBuf>,
        target_path: PathBuf,
        #[source]
        source: nix::Error,
    },

    #[error("Unmount-Fehler: {path}: {source}")]
    Unmount {
        path: PathBuf,
        #[source]
        source: nix::Error,
    },

    #[error("System-Fehler: {0}")]
    System(String),

    #[error("Netzwerk-Fehler: {0}")]
    Network(String),

    #[error("Unbekannter Fehler")]
    Unknown,
}

pub type Result<T> = std::result::Result<T, SkoodaError>;
