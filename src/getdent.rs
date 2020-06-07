use bytemuck::{Pod, Zeroable};

use core::convert::TryFrom;
use core::{mem, ptr};

use index_ext::Int;

use std::io;
use std::ffi;
use std::os::unix::ffi::OsStrExt;

use super::UnixFileType as FileType;

/// A buffer for collecting results of `getdents`.
pub struct DirentBuf {
    inner: Box<[u8]>,
    /// The index of the first set buffer.
    start: usize,
    /// The index of the first free byte.
    last: usize,
}

/// A reference to a single entry.
pub struct Entry<'buf> {
    inner: &'buf Dirent64,
}

/// A consistency error of the result buffer.
pub enum DirentErr {
    TooShort,
    InvalidLength,
}

pub enum More {
    More,
    Blocked,
    Done,
}

impl DirentBuf {
    pub fn with_size(length: usize) -> Self {
        libc::c_uint::try_from(length).expect("Buffer size invalid for `getdent` syscall.");

        DirentBuf {
            inner: vec![0; length].into(),
            start: 0,
            last: 0,
        }
    }

    pub fn iter(&self) -> Entries<'_> {
        Entries {
            remaining: &self.inner[self.start..self.last],
        }
    }

    pub fn drain(&mut self) -> Drain<'_> {
        Drain {
            inner: Entries {
                remaining: &self.inner[self.start..self.last],
            },
            start: &mut self.start,
            last: self.last,
        }
    }

    pub fn fill_buf(&mut self, fd: libc::c_int) -> io::Result<More> {
        // Make buffer as large as possible.
        if self.start == self.last {
            self.start = 0;
            self.last = 0;
        }

        match sys_getdents64(fd, self.get_mut()) {
            0 => Ok(More::Done),
            -1 => {
                match unsafe { *libc::__errno_location() } {
                    libc::EINVAL => Ok(More::Blocked),
                    libc::EFAULT => unreachable!("Buffer outside our memory space"),
                    _ => Err(io::Error::last_os_error())
                }
            },
            other => {
                assert!(other > 0,
                    "Success but negative result.");
                assert!(self.inner[self.last..].get_int(..other).is_some(),
                    "Success but written beyond buffer");
                // The above assert also checks the usize conversion.
                self.last += other as usize;
                Ok(More::More)
            }
        }
    }

    fn get_mut(&mut self) -> &mut DirentTarget {
        // TODO: wait, start position?
        DirentTarget::new(&mut self.inner[self.last..])
    }
}

/// Iterates like entries but removes the entries.
pub struct Entries<'a> {
    remaining: &'a [u8],
}

/// Iterates like entries but removes the entries.
pub struct Drain<'a> {
    inner: Entries<'a>,
    start: &'a mut usize,
    last: usize,
}

impl Entry<'_> {
    pub fn path(&self) -> &ffi::OsStr {
        ffi::OsStr::from_bytes(&self.inner.d_name)
    }

    pub fn file_type(&self) -> Option<FileType> {
        FileType::new(self.inner.d_type)
    }
}

/// The slice into which the kernel should place dirents.
struct DirentTarget {
    align: [dirent64; 0],
    buf: [u8],
}

/// The actual unsized descriptor of the entry.
#[repr(packed)]
struct Dirent64 {
    /// The inode associated with the entry.
    d_ino: libc::c_ulong,
    /// The offset to the next entry, for seeking.
    d_off: libc::c_ulong,
    /// The length of the buffer, _after_ the syscall succeeded.
    d_reclen: libc::c_ushort,
    /// The type indicated by the kernel, or unknown.
    d_type: libc::c_char,
    /// var length name, with the length indicated in `d_reclen`.
    d_name: [u8],
}

/// This is just an ffi descriptor type.
#[allow(non_snake_case, non_camel_case_types)]
// FFI type
// Be careful that this struct is actually zeroable and a Pod. In particular we want to avoid
// having any padding bytes.
#[repr(packed)]
#[derive(Clone, Copy)]
struct dirent64 {
    d_ino: libc::c_ulong,
    d_off: libc::c_ulong,
    /// The length of the buffer, _after_ the syscall succeeded.
    d_reclen: libc::c_ushort,
    /// The type indicated by the kernel, or unknown.
    d_type: libc::c_char,
    /// var length name, but we also have an array of such structs.
    d_name: [libc::c_char; 0],
}

// SAFETY: no padding due to packed.
unsafe impl Zeroable for dirent64 {}
unsafe impl Pod for dirent64 {}

/// Return value is:
/// * `0` if the directory is at the end.
/// * `-1` if there was an error, the error is:
///   * `EINVAL` signals our buffer as too short
///   * `EBADF` if the file descriptor is invalid
///   * `ENOENT` for No such directory
///   * `EFAULT` if the target pointer was outside our address space
///   * `ENOTDIR` if `fd` is not a directory
fn sys_getdents64(fd: libc::c_int, into: &mut DirentTarget) -> libc::c_int {
    let length: libc::c_uint = libc::c_uint::try_from(into.buf.len())
        .expect("Invalid buffer length should have been checked");
    unsafe {
        libc::syscall(
            libc::SYS_getdents64,
            fd,
            into.buf.as_mut_ptr() as *mut libc::c_char,
            length,
        ) as libc::c_int
    }
}

impl Dirent64 {
    fn from_start(buf: &[u8]) -> Result<(&Self, &[u8]), DirentErr> {
        let speculate = buf
            .get(..mem::size_of::<dirent64>())
            .ok_or(DirentErr::TooShort)?;
        let spec_head = bytemuck::from_bytes(speculate);
        let dirent64 { d_reclen, .. } = *spec_head;

        let spec_entry = buf.get_int(..d_reclen).ok_or(DirentErr::InvalidLength)?;
        let tail = buf.get_int(d_reclen..).unwrap();

        // Do a final consistency check.
        let _entry_head = spec_entry
            .get(..mem::size_of::<dirent64>())
            .ok_or(DirentErr::InvalidLength)?;
        let raw_entry_name = spec_entry
            .get(mem::size_of::<dirent64>()..)
            .unwrap();

        let clen = raw_entry_name
            .iter()
            .position(|&b| b == b'\0')
            .ok_or(DirentErr::InvalidLength)?;

        // Did all consistency checks necessary! (The null-byte check can be done later, we'll
        // check for UTF-8 as well so who cares).
        let entry = spec_entry;

        // Now we need to do the DST cast. We give it the provenance information of the complete
        // entry but its slice-length meta information needs to be only the length of the name.
        let ptr = entry as *const [u8];
        // Transfer the name of the length field.
        let raw = ptr::slice_from_raw_parts(ptr as *const u8, clen);
        let entry = unsafe { &*(raw as *const Dirent64) };

        Ok((entry, tail))
    }
}

/// Not a transparent wrapper, as we have an alignment invariant.
impl DirentTarget {
    fn new(buffer: &mut [u8]) -> &mut Self {
        //SAFETY: No extra safety invariants, just a marker type.
        unsafe { &mut *(buffer as *mut [u8] as *mut DirentTarget) }
    }
}

impl<'a> Iterator for Entries<'a> {
    type Item = Result<Entry<'a>, DirentErr>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }

        match Dirent64::from_start(self.remaining) {
            Ok((entry, remaining)) => {
                self.remaining = remaining;
                Some(Ok(Entry { inner: entry }))
            }
            Err(err) => {
                self.remaining = <&'_ [u8]>::default();
                Some(Err(err))
            }
        }
    }
}

impl<'a> Iterator for Drain<'a> {
    type Item = Result<Entry<'a>, DirentErr>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next() {
            Some(Ok(entry)) => {
                let len = unsafe { ptr::read_unaligned(&entry.inner.d_reclen) };
                *self.start += len as usize;
                Some(Ok(entry))
            }
            Some(Err(err)) => {
                *self.start = self.last;
                Some(Err(err))
            }
            None => None,
        }
    }
}
