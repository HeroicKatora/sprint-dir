# sprint-dir

A faster implementation for `walkdir` on Linux and large folder trees,
utilizing the `getdents` system call that is not exposed via `libc` directly.

I do not currently endorse contributions to this repository as the project is a
personal experimentation. Neverthless, if you like the name, and want to build
an even cooler library such as adding `async` support or even using io-uring or
want to expand it cross-platform: Contact me over Github and we may discuss
transferring the crate name.

License: WTFPL
