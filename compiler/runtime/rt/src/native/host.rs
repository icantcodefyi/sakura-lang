//! The native host interface (ABI §1b): what an instance asks of its embedder. The runtime
//! calls it only on the instance's thread and never while a borrow the host could observe is
//! live; the host never receives the context, so it cannot re-enter the runtime. Inputs are
//! borrowed (copy-in by construction), answers are owned (the runtime copies them out).

use alloc::string::String;
use alloc::vec::Vec;

/// A host handle as the guest's `Handle` object names it (ABI §8.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HandleId {
    pub kind: u32,
    pub slot: u32,
    pub generation: u32,
}

/// Why a host operation produced no value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostError {
    /// The operation's ordinary `Err(reason)` (statuses 1 and 2 of ABI §8.2: a refusal under
    /// `[permissions]` is reported the same way, as JS).
    Err(String),
    /// The host failed (status 3): defect `D091` with this message.
    Failure(String),
    /// The guest handed the host something a checked program cannot (a forged or foreign
    /// handle): defect `D090` with this message (ABI §8.4).
    Fault(String),
}

pub type Hosted<T> = Result<T, HostError>;

/// `std::fs::Mode`, the constructor index the guest passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileMode {
    Read,
    Write,
    Append,
}

/// One instance's host. Methods without a default are the root handlers' imports, which every
/// host provides; the `hosted` builtins default to a host failure (`D091`), so a host
/// implements only the services it offers.
pub trait Host {
    fn console_print(&mut self, line: &str);
    /// The next line of input without its newline, `None` at the end.
    fn console_read_line(&mut self) -> Option<String>;
    fn clock_now_ms(&mut self) -> i64;
    fn clock_monotonic_ms(&mut self) -> i64;
    /// `async` (ABI §8.2): complete `request` (`Instance::complete`) after `ms` milliseconds.
    fn clock_sleep_ms(&mut self, request: u32, ms: i64);
    /// Cancel the host side of `request` (ABI §8.5); a later completion is discarded anyway.
    fn abort(&mut self, request: u32);
    /// Uniform in `lo..=hi` (`lo <= hi` is checked by the runtime).
    fn random_int(&mut self, lo: i64, hi: i64) -> i64;
    /// Uniform in `[0, 1)`.
    fn random_float(&mut self) -> f64;
    fn random_bytes(&mut self, n: usize) -> Vec<u8>;
    /// A canonical lowercase version-4 UUID.
    fn random_uuid(&mut self) -> String;
    /// One stderr line (`Log.stderr()`).
    fn log_write(&mut self, line: &str);
    /// Whether effect traces are recorded (`SAKURA_TRACE`), read once at instantiation.
    fn trace_enabled(&mut self) -> bool;
    fn trace_write(&mut self, line: &str);
    /// The guest's last reference to `handle` died (ABI §8.4).
    fn retire_handle(&mut self, handle: HandleId);

    fn env_get(&mut self, _key: &str) -> Hosted<Option<String>> {
        Err(unsupported())
    }
    fn fs_root(&mut self) -> Hosted<HandleId> {
        Err(unsupported())
    }
    fn fs_memory_root(&mut self, _files: Vec<(String, Vec<u8>)>) -> Hosted<HandleId> {
        Err(unsupported())
    }
    fn fs_subdir(&mut self, _dir: HandleId, _name: &str) -> Hosted<HandleId> {
        Err(unsupported())
    }
    fn fs_read(&mut self, _dir: HandleId, _path: &str) -> Hosted<Vec<u8>> {
        Err(unsupported())
    }
    fn fs_write(&mut self, _dir: HandleId, _path: &str, _data: &[u8]) -> Hosted<()> {
        Err(unsupported())
    }
    /// Entries `(name, is_dir)` sorted by name.
    fn fs_list(&mut self, _dir: HandleId, _path: &str) -> Hosted<Vec<(String, bool)>> {
        Err(unsupported())
    }
    fn fs_exists(&mut self, _dir: HandleId, _path: &str) -> Hosted<bool> {
        Err(unsupported())
    }
    /// The id of a new open file (`File.id`).
    fn file_open(&mut self, _dir: HandleId, _path: &str, _mode: FileMode) -> Hosted<i64> {
        Err(unsupported())
    }
    fn file_read(&mut self, _id: i64, _max: i64) -> Hosted<Vec<u8>> {
        Err(unsupported())
    }
    fn file_write(&mut self, _id: i64, _data: &[u8]) -> Hosted<()> {
        Err(unsupported())
    }
    /// Closes file `id`; a second close does nothing.
    fn file_close(&mut self, _id: i64) {}
}

fn unsupported() -> HostError {
    HostError::Failure("a host operation this native host does not provide".into())
}
