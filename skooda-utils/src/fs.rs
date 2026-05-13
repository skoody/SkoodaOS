use crate::error::{Result, SkoodaError};
use std::fs;
use std::path::Path;
use walkdir::{WalkDir, DirEntry};
use tracing::info;

pub trait CopyOps {
    fn copy_recursive<P: AsRef<Path>>(&self, src: P, dst: P, progress: Option<&dyn Fn(u64, u64)>) -> Result<()>;
}

pub struct DefaultFileSystem;

impl CopyOps for DefaultFileSystem {
    fn copy_recursive<P: AsRef<Path>>(&self, src: P, dst: P, progress: Option<&dyn Fn(u64, u64)>) -> Result<()> {
        let src = src.as_ref();
        let dst = dst.as_ref();

        info!("Copying recursively: {:?} -> {:?}", src, dst);

        let mut total_files = 0;
        for _ in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
            total_files += 1;
        }

        let mut current_file = 0;
        for entry in WalkDir::new(src) {
            let entry: DirEntry = entry.map_err(|e| SkoodaError::Io {
                path: src.to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::Other, e),
            })?;

            let rel_path = entry.path().strip_prefix(src).map_err(|_| {
                SkoodaError::System("Path stripping failed".to_string())
            })?;
            let dest_path = dst.join(rel_path);

            if entry.file_type().is_dir() {
                fs::create_dir_all(&dest_path).map_err(|e| SkoodaError::Io {
                    path: dest_path,
                    source: e,
                })?;
            } else if entry.file_type().is_symlink() {
                let target = fs::read_link(entry.path()).map_err(|e| SkoodaError::Io {
                    path: entry.path().to_path_buf(),
                    source: e,
                })?;
                #[cfg(unix)]
                std::os::unix::fs::symlink(target, &dest_path).map_err(|e| SkoodaError::Io {
                    path: dest_path,
                    source: e,
                })?;
            } else {
                fs::copy(entry.path(), &dest_path).map_err(|e| SkoodaError::Io {
                    path: dest_path,
                    source: e,
                })?;
            }

            current_file += 1;
            if let Some(callback) = progress {
                callback(current_file, total_files);
            }
        }

        Ok(())
    }
}

pub fn ensure_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    if !path.exists() {
        fs::create_dir_all(path).map_err(|e| SkoodaError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
    }
    Ok(())
}

pub fn cat<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    fs::read_to_string(path).map_err(|e| SkoodaError::Io {
        path: path.to_path_buf(),
        source: e,
    })
}

pub fn rm<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    fs::remove_file(path).map_err(|e| SkoodaError::Io {
        path: path.to_path_buf(),
        source: e,
    })
}

pub fn mkdir<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    fs::create_dir_all(path).map_err(|e| SkoodaError::Io {
        path: path.to_path_buf(),
        source: e,
    })
}
