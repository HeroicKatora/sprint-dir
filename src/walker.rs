use std::path::Path;

pub struct WalkDir {
    /// Directories for which we have a file descriptor open.
    open: Vec<Open>,
    /// Directories which are currently in the parent hierarchy.
    parents: Vec<Parent>,
}

pub struct Error {
    _private: (),
}

struct Open {}

struct Parent {}

impl WalkDir {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        unimplemented!()
    }
}

impl Error {
    fn new() -> Self {
        Error { _private: () }
    }
}
