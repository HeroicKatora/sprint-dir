use crate::getdent::DirentBuf;

use core::mem;
use std::io;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::ffi::OsStrExt;
use once_cell::sync::OnceCell;

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
    /// The number of file descriptors we are still allowed to open.
    open_budget: usize,
    /// Statistics about the system calls etc.
    stats: Stats,
}

/// Describes a file that was found.
///
/// All parents of this entry have already been yielded before.
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// The file type reported by the call to `getdent`.
    file_type: FileType,
    /// The depth at which this entry was found.
    depth: usize,
    /// The file name of this entry.
    file_name: EntryPath,
    /// The normalized full path of the entry.
    full_path: OnceCell<PathBuf>,
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

#[derive(Debug, Default)]
struct Stats {
    nr_close: usize,
    nr_getdent: usize,
    nr_open: usize,
    nr_openat: usize,
    nr_stat: usize,
}

/// Completed directory nodes that are parents of still open nodes or active entries.
#[derive(Debug)]
struct Node {
    /// The depth at which this node occurs.
    depth: usize,
    /// The path of this node.
    path: EntryPath,
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
        self.config.assert_consistent();
        let first_item = self.initial_closed();

        IntoIter {
            config: self.config,
            stack: vec![WorkItem::Closed(first_item)],
            open_budget: 128,
            stats: Stats::default(),
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

impl Configuration {
    fn assert_consistent(&self) {
        assert!(self.min_depth <= self.max_depth);
        assert!(self.max_open > 0);
        assert!(!self.follow_links, "Unsupported");
        assert!(!self.same_file_system , "Unsupported");
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

    pub fn stats(&self) -> &dyn core::fmt::Debug {
        &self.stats
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

    fn set(&mut self, inner: FileTypeInner) {
        self.inner = Some(inner);
    }
}

impl DirEntry {
    // TODO: enable `openat`?

    /// Inspect the path of this entry.
    pub fn path(&self) -> &Path {
        self.full_path.get_or_init(|| {
            self.file_name.make_path()
        })
    }

    pub fn path_is_symlink(&self) -> bool {
        self.file_type.is_symlink()
    }

    /// Read the full meta data.
    pub fn metadata(&self) -> io::Result<std::fs::Metadata> {
        std::fs::metadata(self.path())
    }

    /// Convert the entry into a path
    ///
    /// Potentially more efficient than `as_path().to_owned()`.
    pub fn into_path(self) -> PathBuf {
        let file_name = self.file_name;
        self.full_path.into_inner().unwrap_or_else(|| {
            file_name.make_path()
        })
    }

    pub fn file_type(&self) -> FileType {
        self.file_type
    }

    /// Return the filename of this entry.
    pub fn file_name(&self) -> &OsStr {
        match &self.file_name {
            EntryPath::Full(buf) => buf.file_name().unwrap(),
            EntryPath::Name { name, .. } => name,
        }
    }

    /// The depth at which this entry is in the directory tree.
    ///
    /// When iterating items in depth-first order and following symbolic links then this is not
    /// necessarily the smallest depth at which it might appear.
    pub fn depth(&self) -> usize {
        self.depth
    }
}

impl Open {
    fn openat_os(&self, path: &OsStr, stats: &mut Stats) -> io::Result<Self> {
        let bytes = path.as_bytes().to_owned();
        let cstr = CString::new(bytes).unwrap();
        self.openat(&cstr, stats)
    }

    fn openat(&self, path: &CStr, stats: &mut Stats) -> io::Result<Self> {
        stats.nr_openat += 1;
        let fd = self.fd.openat(path)?;
        let filename = OsStr::from_bytes(path.to_bytes()).to_owned();

        Ok(Open {
            fd,
            buffer: DirentBuf::with_size(1 << 14),
            depth: self.depth + 1,
            as_parent: Arc::new(Node {
                path: EntryPath::Name {
                    name: filename,
                    parent: self.as_parent.clone(),
                },
                depth: self.depth + 1,
            }),
        })
    }

    /// Get the next item from this directory.
    fn pop(&mut self) -> Option<Entry<'_>> {
        self.buffer.drain().next().map(Self::okay)
    }

    fn ready_entry(&mut self) -> Option<DirEntry> {
        let depth = self.depth;
        let parent = self.as_parent.clone();
        let entry = self.pop()?;

        let entry = match Self::sub_entry(entry) {
            None => return self.ready_entry(),
            Some(entry) => entry,
        };

        Some(DirEntry {
            file_name: EntryPath::Name {
                name: entry.file_name().to_owned(),
                parent,
            },
            depth,
            file_type: FileType {
                inner: entry.file_type(),
            },
            full_path: OnceCell::new(),
        })
    }

    fn fill_buffer(&mut self, stats: &mut Stats) -> io::Result<More> {
        stats.nr_getdent += 1;
        self.buffer.fill_buf(self.fd.0)
    }

    /// Forcibly close this directory entry.
    /// Returns None if its already finished and Some with the remaining backlog items otherwise.
    fn close(mut self, stats: &mut Stats) -> io::Result<Option<Closed>> {
        let mut backlog = vec![];
        let base = self.as_parent.make_path();

        loop {
            let entries = self.buffer
                .drain()
                .map(Self::okay)
                .filter_map(Self::sub_entry)
                .map(|entry| Self::backlog(&base, entry));
            backlog.extend(entries);
            stats.nr_getdent += 1;
            match self.buffer.fill_buf(self.fd.0)? {
                More::Blocked => unreachable!("Just drained buffer is blocked"),
                More::More => {},
                More::Done => break,
            }
        }

        if backlog.is_empty() {
            stats.nr_close += 1;
            self.fd.close()?;
            Ok(None)
        } else {
            let closed = Closed::from_backlog(&self, backlog);
            stats.nr_close += 1;
            self.fd.close()?;
            Ok(Some(closed))
        }
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

    fn sub_entry(entry: Entry<'_>) -> Option<Entry<'_>> {
        // Never recurse into current or parent directory.
        match Path::new(entry.file_name()).components().next() {
            Some(Component::CurDir) | Some(Component::ParentDir) => None,
            _ => Some(entry),
        }

    }

    fn backlog(base: &Path, entry: Entry<'_>) -> Backlog {
        Backlog {
            file_path: base.join(entry.file_name()),
            file_type: entry.file_type(),
        }
    }
}

impl DirFd {
    fn open(path: &Path) -> io::Result<Self> {
        let raw_name = path.as_os_str().as_bytes().to_owned();
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
    fn from_backlog(open: &Open, children: Vec<Backlog>) -> Self {
        Closed {
            depth: open.depth + 1,
            children,
            as_parent: None,
        }
    }

    fn open(&self, backlog: &DirEntry, stats: &mut Stats) -> io::Result<Open> {
        let path = backlog.file_name.make_path();
        stats.nr_open += 1;
        let fd = DirFd::open(&path)?;

        Ok(Open {
            fd,
            buffer: DirentBuf::with_size(1 << 14),
            depth: self.depth + 1,
            as_parent: Arc::new(Node {
                depth: self.depth + 1,
                path: EntryPath::Full(path),
            })
        })
    }

    fn ready_entry(&mut self) -> Option<DirEntry> {
        let backlog = self.children.pop()?;
        Some(DirEntry {
            file_name: EntryPath::Full(backlog.file_path),
            file_type: FileType {
                inner: backlog.file_type
            },
            depth: self.depth,
            full_path: OnceCell::new(),
        })
    }
}

impl EntryPath {
    fn make_path(&self) -> PathBuf {
        match self {
            EntryPath::Full(buf) => buf.clone(),
            EntryPath::Name { name, parent } => {
                let mut buf = parent.make_path();
                buf.push(&name);
                buf
            }
        }
    }
}

impl Node {
    /// Allocate a path buffer for the path described.
    fn make_path(&self) -> PathBuf {
        self.path.make_path()
    }
}

impl IntoIter {
    /// See if we should descend to the newly found entry.
    fn iter_entry(&mut self, entry: &mut DirEntry) -> Result<(), Error> {
        let is_dir = match entry.file_type.inner {
            Some(FileTypeInner::Directory) => true,
            Some(_) => false,
            None => {
                //can we make fstatat work?
                self.stats.nr_stat += 1;
                let meta = std::fs::metadata(entry.file_name.make_path())
                    .map_err(Error::from_io)?
                    .file_type();
                if meta.is_dir() {
                    entry.file_type.set(FileTypeInner::Directory);
                    true
                } else if meta.is_file() {
                    entry.file_type.set(FileTypeInner::File);
                    false
                } else if meta.is_symlink() {
                    entry.file_type.set(FileTypeInner::SymbolicLink);
                    false
                } else if meta.is_block_device() {
                    entry.file_type.set(FileTypeInner::BlockDevice);
                    false
                } else if meta.is_char_device() {
                    entry.file_type.set(FileTypeInner::CharDevice);
                    false
                } else if meta.is_fifo() {
                    entry.file_type.set(FileTypeInner::File);
                    false
                } else if meta.is_socket() {
                    entry.file_type.set(FileTypeInner::UnixSocket);
                    false
                } else {
                    false
                }
            }
        };

        if is_dir {
            // TODO: filter? min_depth? max_depth?

            let can_open = self.open_budget > 0;
            let mut next: WorkItem = match self.stack.last().unwrap() {
                WorkItem::Open(open) if can_open => {
                    open.openat_os(entry.file_name(), &mut self.stats)
                        .map_err(Error::from_io)
                        .map(WorkItem::Open)?
                }
                WorkItem::Open(open) => {
                    if self.config.contents_first {
                        // TODO: close and open the actual next.
                    } else {
                        // TODO: add the sub directory as a closed one.
                    }

                    todo!()
                }
                WorkItem::Closed(closed) => {
                    assert!(can_open, "No more budget but only closed work items");
                    closed.open(entry, &mut self.stats)
                        .map_err(Error::from_io)
                        .map(WorkItem::Open)?
                }
            };

            if !self.config.contents_first {
                mem::swap(&mut next, self.stack.last_mut().unwrap());
            }

            self.stack.push(next);
        }

        Ok({})
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
        let mut current = self.stack.last_mut()?;

        // First try to get an item that is ripe for reaping.
        let mut found = match &mut current {
            WorkItem::Open(open) => match open.ready_entry() {
                Some(entry) => entry,
                // No more items, try refilling.
                None => {
                    match open.fill_buffer(&mut self.stats) {
                        Err(err) => todo!(),
                        Ok(More::More) => return self.next(),
                        Ok(More::Blocked) => unreachable!("Empty buffer blocked"),
                        Ok(More::Done) => {
                            let _ = self.stack.pop();
                            return self.next();
                        }
                    }
                },
            }
            WorkItem::Closed(closed) => match closed.ready_entry() {
                Some(entry) => entry,
                None => {
                    // Nothing to do, try the next entry.
                    let _ = self.stack.pop();
                    return self.next();
                }
            }
        };

        Some(self.iter_entry(&mut found).map(|_| found))
    }
}

// Private implementation items.

impl Open {
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

    fn from_io(_: io::Error) -> Self {
        Error::new()
    }
}

impl<P> Iterator for FilterEntry<IntoIter, P> {
    type Item = Result<DirEntry, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        unimplemented!()
    }
}
