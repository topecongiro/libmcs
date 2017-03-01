// Copyright 2014 The Rust Project Developers.
// Copyright 2017 Seiichi Uchida <uchida@os.ecc.u-tokyo.ac.jp>

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LockResult, PoisonError};
use std::thread;

pub struct Flag { failed: AtomicBool }
pub struct Guard { panicking: bool }

impl Flag {
    pub const fn new() -> Flag {
        Flag { failed: AtomicBool::new(false) }
    }

    #[inline]
    pub fn borrow(&self) -> LockResult<Guard> {
        let ret = Guard { panicking: thread::panicking() };
        if self.get() {
            Err(PoisonError::new(ret))
        } else {
            Ok(ret)
        }
    }

    #[inline]
    pub fn done(&self, guard: &Guard) {
        if !guard.panicking && thread::panicking() {
            self.failed.store(true, Ordering::Relaxed);
        }
    }

    #[inline]
    pub fn get(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }
}

pub fn map_result<T, U, F>(result: LockResult<T>, f: F)
                           -> LockResult<U>
    where F: FnOnce(T) -> U {
    match result {
        Ok(t) => Ok(f(t)),
        Err(e) => Err(PoisonError::new(f(e.into_inner())))
    }
}
