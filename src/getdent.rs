use bytemuck::{Pod, Zeroable};
use core::{mem, ptr};
use index_ext::Int;

pub struct DirentBuf {
    inner: Box<[u8]>,
}

pub struct Entry<'buf> {
    inner: &'buf Dirent64,
}

pub enum DirentErr {
    TooShort,
    InvalidOffset,
    InvalidLength,
}

impl DirentBuf {
    pub fn with_size(length: usize) -> Self {
        DirentBuf {
            inner: vec![0; length].into(),
        }
    }

    pub fn iter(&self) -> Entries<'_> {
        Entries {
            remaining: &*self.inner,
        }
    }
}

pub struct Entries<'a> {
    remaining: &'a [u8],
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
    /// The offset to the next entry.
    d_off: libc::c_ulong,
    /// The length of the buffer, _after_ the syscall succeeded.
    d_reclen: libc::c_ushort,
    /// The type indicated by the kernel, or unknown.
    d_type: libc::c_char,
    /// var length name, with the length indicated in `d_reclen`.
    d_name: [libc::c_char],
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

fn sys_getdents64(fd: libc::c_uint, into: &mut DirentTarget) -> libc::c_int {
    use core::convert::TryFrom;
    // FIXME: handle sizes.
    let length: libc::c_uint = libc::c_uint::try_from(into.buf.len()).unwrap();
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
    pub fn from_start(buf: &[u8]) -> Result<(&Self, &[u8]), DirentErr> {
        let speculate = buf
            .get(..mem::size_of::<dirent64>())
            .ok_or(DirentErr::TooShort)?;
        let spec_head = bytemuck::from_bytes(speculate);
        let dirent64 {
            d_off, d_reclen, ..
        } = *spec_head;

        let head = buf.get_int(..d_off).ok_or(DirentErr::InvalidOffset)?;
        let tail = buf.get_int(d_off..).unwrap();

        let spec_entry = head.get_int(..d_reclen).ok_or(DirentErr::InvalidLength)?;

        // Do a final consistency check.
        let _entry_head = spec_entry
            .get(..mem::size_of::<dirent64>())
            .ok_or(DirentErr::InvalidOffset)?;
        let entry_name = head.get(mem::size_of::<dirent64>()..).unwrap();

        // Did all consistency checks necessary! (The null-byte check can be done later, we'll
        // check for UTF-8 as well so who cares).
        let entry = spec_entry;

        // Now we need to do the DST cast. We give it the provenance information of the complete
        // entry but its slice-length meta information needs to be only the length of the name.
        let ptr = entry as *const [u8];
        // Transfer the name of the length field.
        let raw = ptr::slice_from_raw_parts(ptr as *const u8, entry_name.len());
        let entry = unsafe { &*(raw as *const Dirent64) };

        Ok((entry, tail))
    }
}

/// Not a transparent wrapper, as we have an alignment invariant.
impl DirentTarget {
    fn new(buffer: &mut [u8]) -> Option<&mut Self> {
        // TODO: we'll see if we need alignment invariants.
        Some(unsafe { &mut *(buffer as *mut [u8] as *mut DirentTarget) })
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
