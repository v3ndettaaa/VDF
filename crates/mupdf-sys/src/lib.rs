//! mupdf-sys — raw MuPDF 1.28.5 FFI.
//!
//! Architecture rules (MASTER_PLAN.md §5):
//! - the ONLY consumer of this crate is `vdf-pdf`
//! - MuPDF reports errors via setjmp/longjmp; every fallible call is wrapped
//!   by a C shim (`src/shim.c`) that contains the try/catch in C and returns
//!   0/1 with an error message buffer — no longjmp ever crosses into Rust
//! - thread safety: MuPDF contexts share state through `fz_locks_context`;
//!   this crate provides the spinlock-backed locks (FZ_LOCKS=3) and context
//!   creation/clone; the clone-context pattern lives in `vdf-pdf`

pub mod raw;

use std::ffi::c_char;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

/// Error captured from a MuPDF fz_throw by the C shims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MupdfError(pub String);

impl fmt::Display for MupdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mupdf: {}", self.0)
    }
}

impl std::error::Error for MupdfError {}

/// Error buffer size for shim calls (MuPDF messages are short; 1 KiB is ample).
pub const ERRBUF_LEN: usize = 1024;

/// The maximum lock index (`FZ_LOCK_MAX` in context.h: ALLOC, FREETYPE,
/// GLYPHCACHE).
pub const FZ_LOCKS: usize = 3;

/// Minimal spinlock for MuPDF's internal locks.
///
/// MuPDF holds these for very short critical sections (allocator, freetype,
/// glyph cache) and guarantees acquire ordering across its own lock users,
/// so a spinlock is correct here and avoids `Mutex` guard plumbing (which
/// has no stable unlock-without-guard API) and poisoning concerns.
struct SpinLock(AtomicBool);

impl SpinLock {
    const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    fn lock(&self) {
        while self.0.swap(true, Ordering::Acquire) {
            std::hint::spin_loop();
        }
    }

    /// # Safety
    /// Must only be called by the thread currently holding the lock.
    unsafe fn unlock(&self) {
        self.0.store(false, Ordering::Release);
    }
}

extern "C" fn register_handlers_cb(ctx: *mut raw::fz_context, _user: *mut std::ffi::c_void) {
    unsafe { raw::fz_register_document_handlers(ctx) };
}

extern "C" fn lock_fn(user: *mut std::ffi::c_void, lock: std::ffi::c_int) {
    debug_assert!((lock as usize) < FZ_LOCKS);
    // SAFETY: `user` is the address of the heap-pinned FzLocks owner, valid
    // for the whole lifetime of every context referencing it.
    let locks = unsafe { &*(user as *const FzLocks) };
    locks.spinlocks[lock as usize].lock();
}

extern "C" fn unlock_fn(user: *mut std::ffi::c_void, lock: std::ffi::c_int) {
    debug_assert!((lock as usize) < FZ_LOCKS);
    // SAFETY: as above; MuPDF guarantees lock/unlock pairing.
    let locks = unsafe { &*(user as *const FzLocks) };
    unsafe { locks.spinlocks[lock as usize].unlock() };
}

/// Spinlock-backed `fz_locks_context`.
///
/// Address-stable for the lifetime of the master context (heap-allocated and
/// never moved); MuPDF copies the config struct but calls back through the
/// `user` pointer, which targets this allocation.
pub struct FzLocks {
    spinlocks: Box<[SpinLock; FZ_LOCKS]>,
    config: raw::fz_locks_context,
}

impl FzLocks {
    /// Creates the locks with `config.user` already pointing at `self`.
    fn boxed() -> Box<Self> {
        let mut this = Box::new(Self {
            spinlocks: Box::new([SpinLock::new(), SpinLock::new(), SpinLock::new()]),
            config: raw::fz_locks_context {
                user: std::ptr::null_mut(),
                lock: Some(lock_fn),
                unlock: Some(unlock_fn),
            },
        });
        this.config.user = &*this as *const Self as *mut std::ffi::c_void;
        this
    }

    fn config_ptr(&self) -> *const raw::fz_locks_context {
        &self.config
    }
}

/// A MuPDF master context. Clones share the underlying state and must be
/// dropped before this master is dropped (ordering contract in `vdf-pdf`).
pub struct FzContext {
    ptr: *mut raw::fz_context,
    /// Keeps the lock allocation (and the `user` target) alive; read only by
    /// the C lock callbacks, never from Rust.
    #[allow(dead_code)]
    locks: Box<FzLocks>,
}

// SAFETY: the context is created with fz_locks_context; MuPDF serializes all
// shared state through those locks, so the handle is Send + Sync.
unsafe impl Send for FzContext {}
unsafe impl Sync for FzContext {}

impl FzContext {
    /// Creates a master context with the given MuPDF store budget in bytes.
    pub fn new(store_max: usize) -> Result<Self, MupdfError> {
        let locks = FzLocks::boxed();
        // SAFETY: fz_new_context_imp (the expansion of the fz_new_context
        // macro) does not longjmp; returns NULL on failure. `locks` stays
        // alive as long as `self`; the version string must match the pinned
        // MuPDF (build.rs reads it from version.h).
        let version = concat!(env!("MUPDF_FZ_VERSION"), "\0");
        let ptr = unsafe {
            raw::fz_new_context_imp(
                std::ptr::null(),
                locks.config_ptr(),
                store_max,
                version.as_ptr() as *const c_char,
            )
        };
        if ptr.is_null() {
            return Err(MupdfError("fz_new_context failed".into()));
        }
        // Register the document handlers (pdf, xps, ...) — an explicit call,
        // not static linking magic. It may throw on OOM, so it goes through
        // the protected shim.
        let mut err = [0 as c_char; ERRBUF_LEN];
        let rc = unsafe {
            raw::vdf_call_protected(
                ptr,
                register_handlers_cb,
                std::ptr::null_mut(),
                err.as_mut_ptr(),
                ERRBUF_LEN,
            )
        };
        if rc != 0 {
            let e = take_error(&err);
            unsafe { raw::fz_drop_context(ptr) };
            return Err(e);
        }
        Ok(Self { ptr, locks })
    }

    pub fn as_ptr(&self) -> *mut raw::fz_context {
        self.ptr
    }

    /// Clones the context for use on another thread. The clone shares all
    /// state and must be dropped before `self`.
    pub fn clone_context(&self) -> Result<RawContext, MupdfError> {
        // SAFETY: fz_clone_context does not longjmp; returns NULL on failure.
        let ptr = unsafe { raw::fz_clone_context(self.ptr) };
        if ptr.is_null() {
            return Err(MupdfError("fz_clone_context failed".into()));
        }
        Ok(RawContext { ptr })
    }
}

impl Drop for FzContext {
    fn drop(&mut self) {
        // SAFETY: master context dropped exactly once; clones must already
        // be gone (documented contract of this crate).
        unsafe { raw::fz_drop_context(self.ptr) };
    }
}

/// A cloned `fz_context` (no lock ownership — the master owns the locks).
pub struct RawContext {
    pub ptr: *mut raw::fz_context,
}

unsafe impl Send for RawContext {}
unsafe impl Sync for RawContext {}

impl Drop for RawContext {
    fn drop(&mut self) {
        // SAFETY: clone context dropped before master (crate contract).
        unsafe { raw::fz_drop_context(self.ptr) };
    }
}

/// Converts a shim error buffer into an owned [`MupdfError`].
pub fn take_error(buf: &[c_char; ERRBUF_LEN]) -> MupdfError {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    MupdfError(String::from_utf8_lossy(&bytes).into_owned())
}

/// A fresh zeroed error buffer for a shim call.
pub fn errbuf() -> Box<[c_char; ERRBUF_LEN]> {
    Box::new([0; ERRBUF_LEN])
}
