extern crate libmcs;

use std::str::FromStr;
use std::env;
use std::thread;
use std::sync::Arc;
use std::sync::mpsc;
use std::time;

/// A default value for thread number.
const THREAD_NUM: usize = 4;

/// A default number for number of critical section per thread.
const LOOP: usize = 1000000;

/// Run a simple benchmark.
fn simple_benchmark(use_mcs: bool, thread_num: usize, loop_num: usize) -> f64 {
    let (tx, rx) = mpsc::channel();

    if use_mcs {
        let data = Arc::new(libmcs::Mutex::new(0));

        for _ in 0..thread_num {
            let (data, tx) = (data.clone(), tx.clone());

            thread::spawn(move || {
                let start = time::Instant::now();
                for _ in 0..loop_num {
                    let mut data = data.lock().unwrap();
                    for _ in 0..100 {
                        *data += 1;
                    }
                }
                tx.send(start.elapsed().subsec_nanos()).unwrap();
            });
        }
    } else {
        let data = Arc::new(std::sync::Mutex::new(0));

        for _ in 0..thread_num {
            let (data, tx) = (data.clone(), tx.clone());

            thread::spawn(move || {
                let start = time::Instant::now();
                for _ in 0..loop_num {
                    let mut data = data.lock().unwrap();
                    for _ in 0..100 {
                        *data += 1;
                    }
                }
                tx.send(start.elapsed().subsec_nanos()).unwrap();
            });
        }
    }

    let mut total_time: f64 = 0.0;
    for i in 0..thread_num {
        let time = 0.000000001 * rx.recv().unwrap() as f64;
        println!("Thread {}: {}", i, time);
        total_time += time;
    }
    total_time / thread_num as f64
}

/// A simple parser for command line arguments.
/// Call this in ascending order with respect to `index` since this uses `nth()`
/// on `args` internally and consumes previous elements.
fn parse_args(args: &mut env::Args, index: usize, default: usize) -> usize {
    match args.nth(index) {
        Some(ref n) => match usize::from_str(n) {
            Ok(n) => n,
            Err(_) => default,
        },
        None => default,
    }
}

fn main() {
    let mut args = &mut env::args();
    let thread_num = parse_args(args, 1, THREAD_NUM);
    let loop_num = parse_args(args, 2, LOOP);

    println!(
        "The number of thread: {}, the number of loop: {}",
        thread_num, loop_num
    );
    println!(
        "Elapsed time for std::sync::Mutex is {}",
        simple_benchmark(false, thread_num, loop_num)
    );
    println!(
        "Elapsed time for libmcs::Mutex is {}",
        simple_benchmark(true, thread_num, loop_num)
    );
}
