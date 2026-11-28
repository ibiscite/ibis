//! The operating system calls Ibis's layers share: whole-file private writes,
//! bounded filesystem work, contained parser and worker panics, and the `ibis` command's
//! place on the system.
//!
//! It sits beside [`ibis-core`](../ibis_core/index.html), below everything else, and uses no other Ibis
//! crate, so core can stay pure ([docs/architecture.md](https://github.com/ibiscite/ibis/blob/main/docs/architecture.md#layers)).
//! The Rust API may change in any release before 1.0.

pub mod atomic_file;
pub mod command_install;
pub mod file_io;
pub mod panic_guard;
pub mod sync;
pub mod toggle;
