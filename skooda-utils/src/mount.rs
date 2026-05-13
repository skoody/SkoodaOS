use crate::error::{Result, SkoodaError};
use nix::mount::{mount, umount, MsFlags};
use std::path::Path;
use tracing::info;

pub trait MountOps {
    fn mount_fs<P: AsRef<Path>>(&self, source: Option<P>, target: P, fstype: Option<&str>, flags: MsFlags) -> Result<()>;
    fn umount_fs<P: AsRef<Path>>(&self, target: P) -> Result<()>;
    fn bind_mount<P: AsRef<Path>>(&self, source: P, target: P) -> Result<()>;
}

pub struct DefaultMounter;

impl MountOps for DefaultMounter {
    fn mount_fs<P: AsRef<Path>>(&self, source: Option<P>, target: P, fstype: Option<&str>, flags: MsFlags) -> Result<()> {
        let src_path = source.as_ref().map(|p| p.as_ref().to_path_buf());
        let tgt_path = target.as_ref().to_path_buf();
        
        info!("Mounting {:?} -> {:?} (type: {:?})", src_path, tgt_path, fstype);
        
        mount(
            source.as_ref().map(|p| p.as_ref()),
            target.as_ref(),
            fstype,
            flags,
            None::<&str>,
        ).map_err(|e| SkoodaError::Mount {
            source_path: src_path,
            target_path: tgt_path,
            source: e,
        })
    }

    fn umount_fs<P: AsRef<Path>>(&self, target: P) -> Result<()> {
        let tgt_path = target.as_ref().to_path_buf();
        info!("Unmounting {:?}", tgt_path);
        
        umount(target.as_ref()).map_err(|e| SkoodaError::Unmount {
            path: tgt_path,
            source: e,
        })
    }

    fn bind_mount<P: AsRef<Path>>(&self, source: P, target: P) -> Result<()> {
        self.mount_fs(Some(source), target, None, MsFlags::MS_BIND)
    }
}
