# libmcs

This Rust library provides a mutex api using MCS lock algorithm.  

## Examples

These examples are taken from the documentation of `std::sync::Mutex`.  
It shows that `mcs::Mutex` can be interchangebly used with `std::sync::Mutex`.

```
extern crate libmcs;

use std::sync::Arc;
use std::thread;
use std::sync::mpsc::channel;
 
use libmcs::Mutex;

const N: usize = 10;

// Spawn a few threads to increment a shared variable (non-atomically), and
// let the main thread know once all increments are done.
//
// Here we're using an Arc to share memory among threads, and the data inside
// the Arc is protected with a mutex.
let data = Arc::new(Mutex::new(0));

let (tx, rx) = channel();
for _ in 0..10 {
    let (data, tx) = (data.clone(), tx.clone());
    thread::spawn(move || {
        // The shared state can only be accessed once the lock is held.
        // Our non-atomic increment is safe because we're the only thread
        // which can access the shared state when the lock is held.
        //
        // We unwrap() the return value to assert that we are not expecting
        // threads to ever fail while holding the lock.
        let mut data = data.lock().unwrap();
        *data += 1;
        if *data == N {
            tx.send(()).unwrap();
        }
        // the lock is unlocked here when `data` goes out of scope.
    });
}

rx.recv().unwrap();
```

```
// Recovering from a poisoned mutex

extern crate libmcs;

use std::sync::Arc;
use std::thread;

use libmcs::Mutex;

let lock = Arc::new(Mutex::new(0_u32));
let lock2 = lock.clone();

let _ = thread::spawn(move || -> () {
    // This thread will acquire the mutex first, unwrapping the result of
    // `lock` because the lock has not been poisoned.
    let _guard = lock2.lock().unwrap();

    // This panic while holding the lock (`_guard` is in scope) will poison
    // the mutex.
    panic!();
}).join();

// The lock is poisoned by this point, but the returned result can be
// pattern matched on to return the underlying guard on both branches.
let mut guard = match lock.lock() {
    Ok(guard) => guard,
    Err(poisoned) => poisoned.into_inner(),
};

*guard += 1;
```
## About MCS lock

A queue-based spin lock such as MCS lock is said to provide a better scalability than a
simple spin lock because of its distributed nature. However, a drawback of MCS lock is 
that it traditonally required to pass an explicit arguement, whose type is a pointer to a queue node. 
In Rust syntax, the canonical MCS lock API will look like the following:

```
fn lock(q: *mut qnode);
fn unlock(q: *mut qnode);
```

However, the api of this crate does not pose such restriction because `LockGuard` implicitly
takes care of the queue node. Therefore, it can be used in place of `std::sync::Mutex`.
