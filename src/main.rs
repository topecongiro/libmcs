extern crate libmcs;

use std::thread;
use std::sync::Arc;
use std::sync::mpsc;
use std::time;

const THREAD_NUM: usize = 4;
const LOOP: usize = 1000000;

fn simple_benchmark(use_mcs: bool) -> f64 {
    let (tx, rx) = mpsc::channel();

    if use_mcs {
        let data = Arc::new(libmcs::Mutex::new(0));

        for _ in 0..THREAD_NUM {
            let (data, tx) = (data.clone(), tx.clone());

            thread::spawn(move || {
                let start = time::Instant::now();
                for _ in 0..LOOP {
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

        for _ in 0..THREAD_NUM {
            let (data, tx) = (data.clone(), tx.clone());

            thread::spawn(move || {
                let start = time::Instant::now();
                for _ in 0..LOOP {
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
    for i in 0..THREAD_NUM {
        let time = 0.000000001 * rx.recv().unwrap() as f64;
        println!("Thread {}: {}", i, time);
        total_time += time;
    }
    total_time / THREAD_NUM as f64
}

fn main() {
    println!("The number of thread: {}, the loop: {}", THREAD_NUM, LOOP);
    println!("Elapsed time for std::sync::Mutex is {}", simple_benchmark(false));
    println!("Elapsed time for libmcs::Mutex is {}", simple_benchmark(true));
}
