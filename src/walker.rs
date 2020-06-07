use crate::getdent::DirentBuf;

use std::io;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::os::unix::ffi::OsStrExt;

use super::UnixFileType as FileTypeInner;
use super::getdent::{DirentErr, Entry, More};

/// Configure walking over all files in a directory tree.
pub struct WalkDir {
    /// The user supplied configuration.
    config: Configuration,
    path: PathBuf,
}

/// The main iterator.
pub struct IntoIter {
    /// The user supplied configuration.
    config: Configuration,
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
    file_name: EntryPath,
}

#[derive(Debug, Clone)]
enum EntryPath {
    /// We have already allocate the whole path in its own buffer.
    Full(PathBuf),
    /// The path is given as the filename alone.
    Name {
        name: OsString,
        /// The parent directory of the entry.
        parent: Arc<Node>,
    },
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

#[derive(Copy, Clone)]
struct Configuration {
    min_depth: usize,
    max_depth: usize,
    max_open: usize,
    follow_links: bool,
    contents_first: bool,
    same_file_system: bool,
}

/// Completed directory nodes that are parents of still open nodes or active entries.
#[derive(Debug)]
struct Node {
    depth: usize,
    /// The parent of this node.
    parent: Option<Arc<Node>>,
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
    fd: DirFd,
    /// The buffer for reading entries of this directory.
    buffer: DirentBuf,
    /// The directory depth of this descriptor.
    depth: usize,
    /// The parent representation of this node.
    /// Not to be confused with the potentially still open parent directory.
    as_parent: Arc<Node>,
}

/// Describes a directory that had to be closed, and its entries read to memory.
struct Closed {
    /// The directory depth of the directory.
    depth: usize,
    /// The children.
    children: Vec<Backlog>,
    /// The parent representation of this node.
    /// The parent directory is also surely closed but children might not be.
    as_parent: Option<Arc<Node>>,
}

struct DirFd(libc::c_int);

/// Describes an item of a closed directory.
///
/// The directories represented by this type are no-one's parent yet.
///
/// Note that by using `openat` we can avoid having to construct the complete path as a single
/// `PathBuf` but this requires keeping the parent `fd` open.
///
/// TODO: what if we use a dequeue to actually allocate these consecutively in memory?
struct Backlog {
    /// The complete path up to here.
    /// Since the file descriptor was closed we can't use `openat` but need to reconstruct the full
    /// path. We might want to track statistics on this since it really is annoying.
    file_path: PathBuf,
    file_type: Option<FileTypeInner>,
}

// Public interfaces.

impl WalkDir {
    pub fn new(path: impl AsRef<Path>) -> Self {
        WalkDir {
            config: Configuration::default(),
            path: path.as_ref().to_owned(),
        }
    }

    pub fn min_depth(mut self, n: usize) -> Self {
        self.config.min_depth = n;
        self
    }

    pub fn max_depth(mut self, n: usize) -> Self {
        self.config.max_depth = n;
        self
    }

    pub fn max_open(mut self, n: usize) -> Self {
        self.config.max_open = n;
        self
    }

    pub fn follow_links(mut self, yes: bool) -> Self {
        self.config.follow_links = yes;
        self
    }

    pub fn sort_by<F>(self, cmp: F) -> Self where
        F: FnMut(&DirEntry, &DirEntry) -> core::cmp::Ordering + Send + Sync + 'static,
    {
        todo!()
    }

    pub fn contents_first(mut self, yes: bool) -> Self {
        self.config.contents_first = yes;
        self
    }

    pub fn same_file_system(mut self, yes: bool) -> Self {
        self.config.same_file_system = yes;
        self
    }

    pub fn build(mut self) -> IntoIter {
        let first_item = self.initial_closed();

        IntoIter {
            config: self.config,
            stack: vec![WorkItem::Closed(first_item)],
            open_budget: 128,
        }
    }

    fn initial_closed(&mut self) -> Closed {
        let backlog = Backlog {
            file_path: core::mem::take(&mut self.path),
            // We do not _know_ this file type yet, recover and check on iteration.
            file_type: None,
        };

        Closed {
            depth: 0,
            children: vec![backlog],
            as_parent: None,
        }
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            min_depth: 0,
            max_depth: usize::MAX,
            max_open: 10,
            follow_links: false,
            contents_first: false,
            same_file_system: false,
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
    fn pop(&mut self) -> Option<Entry<'_>> {
        self.buffer.drain().next().map(Self::okay)
    }

    fn ready_entry(&mut self) -> Option<DirEntry> {
        let entry = self.pop()?;
        Some(DirEntry {
            file_name: EntryPath::Name {
                name: entry.path().to_owned(),
                parent: self.as_parent.clone(),
            },
            file_type: FileType {
                inner: entry.file_type(),
            },
        })
    }

    /// Forcibly close this directory entry.
    /// Returns None if its already finished and Some with the remaining backlog items otherwise.
    fn close(mut self) -> io::Result<Option<Closed>> {
        let mut backlog = vec![];
        let base = self.make_path();

        loop {
            let entries = self.buffer
                .drain()
                .map(|entry| Self::backlog(&base, entry));
            backlog.extend(entries);
            match self.buffer.fill_buf(self.fd.0)? {
                More::Blocked => unreachable!("Just drained buffer is blocked"),
                More::More => {},
                More::Done => break,
            }
        }

        if backlog.is_empty() {
            self.fd.close()?;
            Ok(None)
        } else {
            let closed = Closed::from_backlog(&self, backlog);
            self.fd.close()?;
            Ok(Some(closed))
        }
    }

    /// Reconstruct the complete path buffer.
    fn make_path(&self) -> PathBuf {
        self.as_parent.path()
    }

    /// Filter an entry that we got from the internal buffer.
    /// Handles kernel errors and setup faults which mustn't occur in regular operation.
    fn okay(entry: Result<Entry<'_>, DirentErr>) -> Entry<'_> {
        match entry {
            Ok(entry) => entry,
            Err(DirentErr::TooShort) => unreachable!("Inconsistent buffer state"),
            Err(DirentErr::InvalidLength) => unreachable!("You must have hit a kernel bug!"),
        }
    }

    fn backlog(base: &Path, entry: Result<Entry<'_>, DirentErr>) -> Backlog {
        let entry = Self::okay(entry);
        Backlog {
            file_path: base.join(entry.path()),
            file_type: entry.file_type(),
        }
    }
}

impl DirFd {
    fn open(path: &Path) -> io::Result<Self> {
        let mut raw_name = path.as_os_str().as_bytes().to_owned();
        raw_name.push(b'\0');
        let unix_name = CString::new(raw_name).expect("No interior NULL byte in Path");

        let result = unsafe {
            libc::open(unix_name.as_c_str().as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY)
        };

        if result == -1 {
            return Err(io::Error::last_os_error());
        }

        Ok(DirFd(result))
    }

    fn openat(&self, path: &CStr) -> io::Result<Self> {
        let result = unsafe {
            libc::openat(self.0, path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY)
        };

        if result == -1 {
            return Err(io::Error::last_os_error());
        }

        Ok(DirFd(result))
    }

    fn close(self) -> io::Result<()> {
        match unsafe { libc::close(self.0) } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
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
            file_name: EntryPath::Full(backlog.file_path),
            file_type: FileType { inner: backlog.file_type },
        })
    }
}

impl Node {
    /// Allocate a path buffer for the path described.
    fn path(&self) -> PathBuf {
        if let Some(parent) = &self.parent {
            let mut buf = parent.path();
            buf.push(&self.filename);
            buf
        } else {
            PathBuf::from(&self.filename)
        }
    }
}

impl IntoIterator for WalkDir {
    type IntoIter = IntoIter;
    type Item = Result<DirEntry, Error>;
    fn into_iter(self) -> IntoIter {
        WalkDir::build(self)
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
    fn openat(&self, path: &CStr) -> io::Result<Self> {
        let fd = self.fd.openat(path)?;

        Ok(Open {
            fd,
            buffer: DirentBuf::with_size(1 << 12),
            depth: self.depth + 1,
            as_parent: todo!(),
        })
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
