mod getdent;
mod walker;
#[cfg(test)]
mod tests;

pub use walker::{DirEntry, Error, FilterEntry, IntoIter, WalkDir};

#[derive(Clone, Copy, Debug, PartialEq)]
enum UnixFileType {
    BlockDevice = 1,
    CharDevice,
    Directory,
    NamedPipe,
    SymbolicLink,
    File,
    UnixSocket,
}

impl UnixFileType {
    fn new(kind: libc::c_char) -> Option<Self> {
        match kind as u8 {
            libc::DT_BLK => Some(Self::BlockDevice),
            libc::DT_CHR => Some(Self::CharDevice),
            libc::DT_DIR => Some(Self::Directory),
            libc::DT_FIFO => Some(Self::NamedPipe),
            libc::DT_LNK => Some(Self::SymbolicLink),
            libc::DT_REG => Some(Self::File),
            libc::DT_SOCK => Some(Self::UnixSocket),
            // Actually, we'd expect DT_UNKNOWN but this doesn't hurt.
            _ => None,
        }
    }
}
