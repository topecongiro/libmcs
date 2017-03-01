// Copyright 2014 The Rust Project Developers.
// Copyright 2017 Seiichi Uchida <uchida@os.ecc.u-tokyo.ac.jp>

use std::cell::UnsafeCell;
use std::fmt;
use std::marker;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr};
use std::sync::atomic::Ordering::{Acquire, Release, Relaxed};
use std::sync::{LockResult, TryLockError, TryLockResult};

use poison;

/// A queue node which represents a waiting thread in MCS lock algorithm.
struct Node {
    next: AtomicPtr<Node>,
    waiting: AtomicBool,
}

impl Node {
    pub fn new() -> Node {
        Node{
            next: AtomicPtr::new(ptr::null_mut()),
            waiting: AtomicBool::new(true),
        }
    }
}

/// A mutual exclusion primitive useful for protecting shared data
///
/// This mutex is based on the MCS lock algorithm.
pub struct Mutex<T: ?Sized> {
    tail: AtomicPtr<Node>,
    poison: poison::Flag,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for Mutex<T> { }
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> { }

/// An RAII implementation of a "scoped lock" of a mutex. When this structure is
/// dropped (falls out of scope), the lock will be unlocked.
pub struct MutexGuard<'a, T: ?Sized + 'a> {
    __lock: &'a Mutex<T>,
    __node: AtomicPtr<Node>,
    __poison: poison::Guard,
}

impl<'a, T: ?Sized> !marker::Send for MutexGuard<'a, T> { }

impl<T> Mutex<T> {
    /// Creates a new mutex in an unlocked state ready for use.
    pub fn new(t: T) -> Mutex<T> {
        Mutex {
            tail: AtomicPtr::new(ptr::null_mut()),
            poison: poison::Flag::new(),
            data: UnsafeCell::new(t),
        }
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Acquires a mutex, blocking the current thread until it is able to do so.
    pub fn lock(&self) -> LockResult<MutexGuard<T>> {
        unsafe {
            let node = Box::into_raw(Box::new(Node::new()));
            let prev = self.tail.swap(node, Acquire);
            if !prev.is_null() {
                (*prev).next.store(node, Relaxed);
                while (*node).waiting.load(Acquire) {}
            }

            MutexGuard::new(self, AtomicPtr::new(node))
        }
    }

    /// Attempts to acquire the lock.
    pub fn try_lock(&self) -> TryLockResult<MutexGuard<T>> {
        unsafe {
            let node = Box::into_raw(Box::new(Node::new()));
            let prev = self.tail.load(Acquire);
            if prev.is_null() {
                let prev_tail = self.tail.compare_and_swap(prev, node, Acquire);
                if prev_tail.is_null() {
                    return Ok(MutexGuard::new(self, AtomicPtr::new(node))?)
                }
            }

            Err(TryLockError::WouldBlock)
        }
    }

    /// Determines whether the lock is poisoned.
    pub fn is_poisoned(&self) -> bool {
        self.poison.get()
    }

    /// Consumes this mutex, returning the underlying data.
    pub fn into_inner(self) -> LockResult<T> where T: Sized {
        // We know statically that there are no outstanding references to
        // `self` so there's no need to lock the inner lock. Moreover,
        // in MCS lock tail points to a null pointer if the lock is not acquired.
        // Therefore, we do not need to take a reference to tail.
        //
        // To get the inner value, we'd like to call `data.into_inner()`,
        // but because `Mutex` impl-s `Drop`, we can't move out of it, so
        // we'll have to destructure it manually instead.
        unsafe {
            let (poison, data) = {
                let ref poison = self.poison;
                let ref data = self.data;
                (ptr::read(poison), ptr::read(data))
            };
            mem::forget(self);

            poison::map_result(poison.borrow(), |_| data.into_inner())
        }
    }

    /// Returns a mutable reference to the underlying data.
    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        let data = unsafe { &mut *self.data.get() };
        poison::map_result(self.poison.borrow(), |_| data)
    }
}

impl<T: ?Sized> Drop for Mutex<T> {
    // Nothing to do since if the lock is not acquired than `self.tail`
    // points to a pointer.
    fn drop(&mut self) { }
}

impl<T: ?Sized + Default> Default for Mutex<T> {
    /// Creates a `Mutex<T>`, with the `Default` value for T.
    fn default() -> Mutex<T> {
        Mutex::new(Default::default())
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.try_lock() {
            Ok(guard) => write!(f, "Mutex {{ datea: {:?} }}", &*guard),
            Err(TryLockError::Poisoned(err)) => {
                write!(f, "Mutex {{ data: Poisoned({:?}) }}", &**err.get_ref())
            },
            Err(TryLockError::WouldBlock) => write!(f, "Mutex {{ <locked> }}")
        }
    }
}

impl<'mutex, T: ?Sized> MutexGuard<'mutex, T> {
    unsafe fn new(lock: &'mutex Mutex<T>, node: AtomicPtr<Node>) -> LockResult<MutexGuard<'mutex, T>> {
        poison::map_result(lock.poison.borrow(), |guard| {
            MutexGuard {
                __lock: lock,
                __node: node,
                __poison: guard,
            }
        })
    }
}

impl<'mutex, T: ?Sized> Deref for MutexGuard<'mutex, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.__lock.data.get() }
    }
}

impl<'mutex, T: ?Sized> DerefMut for MutexGuard<'mutex, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.__lock.data.get() }
    }
}


impl<'mutex, T: ?Sized> Drop for MutexGuard<'mutex, T> {
    fn drop(&mut self) {
        unsafe {
            self.__lock.poison.done(&self.__poison);
            let raw_node_ptr = *self.__node.get_mut();
            let mut succ = (*raw_node_ptr).next.load(Relaxed);
            if succ.is_null() {
                if self.__lock.tail.compare_and_swap(raw_node_ptr, ptr::null_mut(), Release) == raw_node_ptr {
                    // Destroy the node pointer handled by this MutexGuard
                    Box::from_raw(raw_node_ptr);
                    return
                }
                while succ.is_null() {
                    succ = (*raw_node_ptr).next.load(Relaxed);
                }
            }
            (*succ).waiting.store(false, Relaxed);
            Box::from_raw(raw_node_ptr);
        }
    }
}

impl<'mutex, T: ?Sized + fmt::Debug> fmt::Debug for MutexGuard<'mutex, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("MutexGuard")
            .field("lock", &self.__lock)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use super::Mutex;
    use std::sync::mpsc;
    use std::thread;

    #[derive(Eq, PartialEq, Debug)]
    struct NonCopy(i32);

    #[test]
    fn smoke() {
        let m = Mutex::new(());
        drop(m.lock().unwrap());
        drop(m.lock().unwrap());
    }

    #[test]
    fn lots_and_lots() {
        const J: u32 = 1000;
        const K: u32 = 3;

        let m = Arc::new(Mutex::new(0));

        fn inc(m: &Mutex<u32>) {
            for _ in 0..J {
                *m.lock().unwrap() += 1;
            }
        }

        let (tx, rx) = mpsc::channel();
        for _ in 0..K {
            let tx2 = tx.clone();
            let m2 = m.clone();
            thread::spawn(move|| { inc(&m2); tx2.send(()).unwrap(); });
            let tx2 = tx.clone();
            let m2 = m.clone();
            thread::spawn(move|| {inc(&m2); tx2.send(()).unwrap(); });
        }

        drop(tx);
        for _ in 0..2 * K {
            rx.recv().unwrap();
        }
        assert_eq!(*m.lock().unwrap(), J * K * 2);
    }

    #[test]
    fn try_lock() {
        let m = Mutex::new(());
        *m.try_lock().unwrap() = ();
    }

    #[test]
    fn test_into_inner() {
        let m = Mutex::new(NonCopy(10));
        assert_eq!(m.into_inner().unwrap(), NonCopy(10));
    }

    #[test]
    fn test_into_inner_drop() {
        struct Foo(Arc<AtomicUsize>);
        impl Drop for Foo {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let num_drops = Arc::new(AtomicUsize::new(0));
        let m = Mutex::new(Foo(num_drops.clone()));
        assert_eq!(num_drops.load(Ordering::SeqCst), 0);
        {
            let _inner = m.into_inner().unwrap();
            assert_eq!(num_drops.load(Ordering::SeqCst), 0);
        }
        assert_eq!(num_drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_into_inner_poison() {
        let m = Arc::new(Mutex::new(NonCopy(10)));
        let m2 = m.clone();
        let _ = thread::spawn(move || {
            let _lock = m2.lock().unwrap();
            panic!("test panic in inner thread to poison mutex");
        }).join();

        assert!(m.is_poisoned());
        match Arc::try_unwrap(m).unwrap().into_inner() {
            Err(e) => assert_eq!(e.into_inner(), NonCopy(10)),
            Ok(x) => panic!("into_inner of poisoned Mutex is Ok: {:?}", x),
        }
    }

    #[test]
    fn test_get_mut() {
        let mut m = Mutex::new(NonCopy(10));
        *m.get_mut().unwrap() = NonCopy(20);
        assert_eq!(m.into_inner().unwrap(), NonCopy(20));
    }

    #[test]
    fn test_get_mut_poison() {
        let m = Arc::new(Mutex::new(NonCopy(10)));
        let m2 = m.clone();
        let _ = thread::spawn(move || {
            let _lock = m2.lock().unwrap();
            panic!("test panic in inner thread to poison mutex");
        }).join();

        assert!(m.is_poisoned());
        match Arc::try_unwrap(m).unwrap().get_mut() {
            Err(e) => assert_eq!(*e.into_inner(), NonCopy(10)),
            Ok(x) => panic!("get_mut of poisoned Mutex is Ok: {:?}", x),
        }
    }


    #[test]
    fn test_mutex_arc_poison() {
        let arc = Arc::new(Mutex::new(1));
        assert!(!arc.is_poisoned());
        let arc2 = arc.clone();
        let _ = thread::spawn(move|| {
            let lock = arc2.lock().unwrap();
            assert_eq!(*lock, 2);
        }).join();
        assert!(arc.lock().is_err());
        assert!(arc.is_poisoned());
    }

    #[test]
    fn test_mutex_arc_nested() {
        // Tests nested mutexes and access
        // to underlying data.
        let arc = Arc::new(Mutex::new(1));
        let arc2 = Arc::new(Mutex::new(arc));
        let (tx, rx) = mpsc::channel();
        let _t = thread::spawn(move|| {
            let lock = arc2.lock().unwrap();
            let lock2 = lock.lock().unwrap();
            assert_eq!(*lock2, 1);
            tx.send(()).unwrap();
        });
        rx.recv().unwrap();
    }

    #[test]
    fn test_mutex_arc_access_in_unwind() {
        let arc = Arc::new(Mutex::new(1));
        let arc2 = arc.clone();
        let _ = thread::spawn(move|| -> () {
            struct Unwinder {
                i: Arc<Mutex<i32>>,
            }
            impl Drop for Unwinder {
                fn drop(&mut self) {
                    *self.i.lock().unwrap() += 1;
                }
            }
            let _u = Unwinder { i: arc2 };
            panic!();
        }).join();
        let lock = arc.lock().unwrap();
        assert_eq!(*lock, 2);
    }

    #[test]
    fn test_mutex_unsized() {
        let mutex: &Mutex<[i32]> = &Mutex::new([1, 2, 3]);
        {
            let b = &mut *mutex.lock().unwrap();
            b[0] = 4;
            b[2] = 5;
        }
        let comp: &[i32] = &[4, 2, 5];
        assert_eq!(&*mutex.lock().unwrap(), comp);
    }
}