use crate::getdent::DirentBuf;

use std::io;
use std::ffi::{CStr, OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::UnixFileType as FileTypeInner;
use super::getdent::{DirentErr, Entry, More};

/// Configure walking over all files in a directory tree.
pub struct WalkDir {
    init: Open,
}

/// The main iterator.
pub struct IntoIter {
    /// The current 'finger' within the tree of directories.
    stack: Vec<WorkItem>,
    open_budget: usize,
}

/// Describes a file that was found.
///
/// All parents of this entry have already been yielded before.
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// The file type reported by the call to `getdent`.
    file_type: FileType,
    /// The file name of this entry.
    file_name: OsString,
    /// The parent directory of the entry.
    parent: Arc<Node>,
}

#[derive(Debug)]
pub struct Error {
    _private: (),
}

/// The type of a file entry.
///
/// Accessing this will not cause any system calls and is very cheap. However, the type may not
/// always be known. In these cases you need to manually query the file meta data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FileType {
    inner: Option<FileTypeInner>,
}

/// Completed directory nodes that are parents of still open nodes or active entries.
#[derive(Debug)]
struct Node {
    depth: usize,
    /// The parent of this node.
    as_parent: Option<Arc<Node>>,
    /// The file name of this file itself.
    filename: OsString,
}

enum WorkItem {
    /// A directory which is still open.
    Open(Open),
    /// A directory that was closed.
    Closed(Closed),
}

/// Directories with a file descriptor.
struct Open {
    /// The open file descriptor.
    fd: libc::c_int,
    /// The buffer for reading entries of this directory.
    buffer: DirentBuf,
    /// The directory depth of this descriptor.
    depth: usize,
    /// The parent representation of this node.
    /// Not to be confused with the potentially still open parent directory.
    parent: Arc<Node>,
}

/// Describes a directory that had to be closed, and its entries read to memory.
struct Closed {
    /// The complete path up to here.
    /// Since the file descriptor was closed we can't use `openat` but need to reconstruct the full
    /// path. We might want to track statistics on this since it really is annoying.
    path: PathBuf,
    /// The directory depth of the directory.
    depth: usize,
    /// The children.
    children: Vec<Backlog>,
    /// The parent representation of this node.
    /// The parent directory is also surely closed but children might not be.
    parent: Arc<Node>,
}

/// Describes an item of a closed directory.
///
/// The directories represented by this type are no-one's parent yet.
///
/// Note that by using `openat` we can avoid having to construct the complete path as a single
/// `PathBuf` but this requires keeping the parent `fd` open.
///
/// TODO: what if we use a dequeue to actually allocate these consecutively in memory?
struct Backlog {
    file_name: OsString,
    file_type: Option<FileTypeInner>,
}

// Public interfaces.

impl WalkDir {
    pub fn new(path: impl AsRef<Path>) -> Self {
        todo!()
    }

    pub fn min_depth(self, n: usize) -> Self {
        todo!()
    }

    pub fn max_depth(self, n: usize) -> Self {
        todo!()
    }

    pub fn max_open(self, n: usize) -> Self {
        assert!(n > 0, "");
        todo!()
    }

    pub fn follow_links(self, yes: bool) -> Self {
        todo!()
    }

    pub fn sort_by<F>(self, cmp: F) -> Self where
        F: FnMut(&DirEntry, &DirEntry) -> core::cmp::Ordering + Send + Sync + 'static,
    {
        todo!()
    }

    pub fn contents_first(self, yes: bool) -> Self {
        todo!()
    }

    pub fn same_file_system(self, yes: bool) -> Self {
        todo!()
    }

    pub fn into_iter(self) -> IntoIter {
        IntoIter {
            stack: vec![WorkItem::Open(self.init)],
            open_budget: 128,
        }
    }
}

impl IntoIter {
    pub fn skip_current_dir(&mut self) {
        todo!()
    }

    pub fn filter_entry<P>(self, predicate: P) -> FilterEntry<Self, P> where
        P: FnMut(&DirEntry) -> bool,
    {
        todo!()
    }
}

pub struct FilterEntry<I, P> {
    unused: core::marker::PhantomData<(I, P)>,
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        self.inner == Some(FileTypeInner::Directory)
    }

    pub fn is_file(&self) -> bool {
        self.inner == Some(FileTypeInner::File)
    }

    pub fn is_symlink(&self) -> bool {
        self.inner == Some(FileTypeInner::SymbolicLink)
    }
}

impl DirEntry {
    // TODO: enable `openat`?

    /// Inspect the path of this entry.
    pub fn path(&self) -> &Path {
        todo!()
    }

    pub fn path_is_symlink(&self) -> bool {
        todo!()
    }

    /// Read the full meta data.
    pub fn metadata(&self) -> io::Result<std::fs::Metadata> {
        todo!()
    }

    /// Convert the entry into a path
    ///
    /// Potentially more efficient than `as_path().to_owned()`.
    pub fn into_path(self) -> PathBuf {
        todo!()
    }

    pub fn file_type(&self) -> FileType {
        todo!()
    }

    /// Return the filename of this entry.
    pub fn file_name(&self) -> &OsStr {
        todo!()
    }

    /// The depth at which this entry is in the directory tree.
    ///
    /// When iterating items in depth-first order and following symbolic links then this is not
    /// necessarily the smallest depth at which it might appear.
    pub fn depth(&self) -> usize {
        todo!()
    }
}

impl Open {
    /// Get the next item from this directory.
    fn pop(&mut self) -> Option<Backlog> {
        self.buffer.drain().next().map(Self::from_dirent)
    }

    fn ready_entry(&mut self) -> Option<DirEntry> {
        let backlog = self.pop()?;
        Some(DirEntry {
            file_name: backlog.file_name,
            file_type: FileType { inner: backlog.file_type },
            parent: self.parent.clone(),
        })
    }

    /// Forcibly close this directory entry.
    /// Returns None if its already finished and Some with the remaining backlog items otherwise.
    fn close(&mut self) -> io::Result<Option<Closed>> {
        let mut backlog = vec![];
        loop {
            let entries = self.buffer
                .drain()
                .map(Self::from_dirent);
            backlog.extend(entries);
            match self.buffer.fill_buf(self.fd)? {
                More::Blocked => unreachable!("Just drained buffer is blocked"),
                More::More => {},
                More::Done => break,
            }
        }

        match unsafe { libc::close(self.fd) } {
            0 => {},
            _ => return Err(io::Error::last_os_error()),
        };

        if backlog.is_empty() {
            return Ok(None)
        }

        Ok(Some(Closed::from_backlog(self, backlog)))
    }

    fn from_dirent(entry: Result<Entry<'_>, DirentErr>) -> Backlog {
        match entry {
            Ok(entry) => Backlog {
                file_name: entry.path().to_owned(),
                file_type: entry.file_type(),
            },
            Err(DirentErr::TooShort) => unreachable!("Inconsistent buffer state"),
            Err(DirentErr::InvalidLength) => unreachable!("You must have hit a kernel bug!"),
        }
    }
}

impl Closed {
    fn from_backlog(open: &Open, backlog: Vec<Backlog>) -> Self {
        todo!()
    }

    fn ready_entry(&mut self) -> Option<DirEntry> {
        let backlog = self.children.pop()?;
        Some(DirEntry {
            file_name: backlog.file_name,
            file_type: FileType { inner: backlog.file_type },
            parent: self.parent.clone(),
        })
    }
}

impl IntoIterator for WalkDir {
    type IntoIter = IntoIter;
    type Item = Result<DirEntry, Error>;
    fn into_iter(self) -> IntoIter {
        WalkDir::into_iter(self)
    }
}

impl Iterator for IntoIter {
    type Item = Result<DirEntry, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        let mut current = self.stack.pop()?;

        // First try to get an item that is ripe for reaping.
        match &mut current {
            WorkItem::Open(open) => match open.ready_entry() {
                Some(entry) => {
                    // Cleanup the current.
                    self.stack.push(current);
                    return Some(Ok(entry))
                },
                None => {},
            }
            WorkItem::Closed(closed) => match closed.ready_entry() {
                Some(entry) => {
                    // Cleanup the current.
                    self.stack.push(current);
                    return Some(Ok(entry))
                }
                None => {
                    // Nothing to do, try the next entry.
                    return self.next();
                }
            }
        }

        todo!()
    }
}

// Private implementation items.

impl Open {
    /// Open a relative directory.
    fn open_dir_at(&self, path: &CStr) -> Result<libc::c_int, io::Error> {
        let result = unsafe {
            libc::openat(self.fd, path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY)
        };

        if result == -1 {
            return Err(io::Error::last_os_error());
        }

        Ok(result)
    }
}

impl Error {
    fn new() -> Self {
        Error { _private: () }
    }

    pub fn path(&self) -> Option<&Path> {
        todo!()
    }

    pub fn loop_ancestor(&self) -> Option<&Path> {
        todo!()
    }

    pub fn depth(&self) -> usize {
        todo!()
    }

    pub fn io_error(&self) -> Option<&std::io::Error> {
        todo!()
    }

    pub fn into_io_error(&self) -> Option<std::io::Error> {
        todo!()
    }
}

impl<P> Iterator for FilterEntry<IntoIter, P> {
    type Item = Result<DirEntry, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        unimplemented!()
    }
}
