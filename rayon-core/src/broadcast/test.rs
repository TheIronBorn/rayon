#![cfg(test)]

use crate::ThreadPoolBuilder;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::{thread, time};

#[test]
fn broadcast_global() {
    let v = crate::broadcast(|ctx| ctx.index());
    assert!(v.into_iter().eq(0..crate::current_num_threads()));
}

#[test]
fn spawn_broadcast_global() {
    let (tx, rx) = crossbeam_channel::unbounded();
    crate::spawn_broadcast(move |ctx| tx.send(ctx.index()).unwrap());

    let mut v: Vec<_> = rx.into_iter().collect();
    v.sort_unstable();
    assert!(v.into_iter().eq(0..crate::current_num_threads()));
}

#[test]
fn broadcast_pool() {
    let pool = ThreadPoolBuilder::new().num_threads(7).build().unwrap();
    let v = pool.broadcast(|ctx| ctx.index());
    assert!(v.into_iter().eq(0..7));
}

#[test]
fn spawn_broadcast_pool() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let pool = ThreadPoolBuilder::new().num_threads(7).build().unwrap();
    pool.spawn_broadcast(move |ctx| tx.send(ctx.index()).unwrap());

    let mut v: Vec<_> = rx.into_iter().collect();
    v.sort_unstable();
    assert!(v.into_iter().eq(0..7));
}

#[test]
fn broadcast_self() {
    let pool = ThreadPoolBuilder::new().num_threads(7).build().unwrap();
    let v = pool.install(|| crate::broadcast(|ctx| ctx.index()));
    assert!(v.into_iter().eq(0..7));
}

#[test]
fn spawn_broadcast_self() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let pool = ThreadPoolBuilder::new().num_threads(7).build().unwrap();
    pool.spawn(|| crate::spawn_broadcast(move |ctx| tx.send(ctx.index()).unwrap()));

    let mut v: Vec<_> = rx.into_iter().collect();
    v.sort_unstable();
    assert!(v.into_iter().eq(0..7));
}

#[test]
fn broadcast_mutual() {
    let count = AtomicUsize::new(0);
    let pool1 = ThreadPoolBuilder::new().num_threads(3).build().unwrap();
    let pool2 = ThreadPoolBuilder::new().num_threads(7).build().unwrap();
    pool1.install(|| {
        pool2.broadcast(|_| {
            pool1.broadcast(|_| {
                count.fetch_add(1, Ordering::Relaxed);
            })
        })
    });
    assert_eq!(count.into_inner(), 3 * 7);
}

#[test]
fn spawn_broadcast_mutual() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let pool1 = Arc::new(ThreadPoolBuilder::new().num_threads(3).build().unwrap());
    let pool2 = ThreadPoolBuilder::new().num_threads(7).build().unwrap();
    pool1.spawn({
        let pool1 = Arc::clone(&pool1);
        move || {
            pool2.spawn_broadcast(move |_| {
                let tx = tx.clone();
                pool1.spawn_broadcast(move |_| tx.send(()).unwrap())
            })
        }
    });
    assert_eq!(rx.into_iter().count(), 3 * 7);
}

#[test]
fn broadcast_mutual_sleepy() {
    let count = AtomicUsize::new(0);
    let pool1 = ThreadPoolBuilder::new().num_threads(3).build().unwrap();
    let pool2 = ThreadPoolBuilder::new().num_threads(7).build().unwrap();
    pool1.install(|| {
        thread::sleep(time::Duration::from_secs(1));
        pool2.broadcast(|_| {
            thread::sleep(time::Duration::from_secs(1));
            pool1.broadcast(|_| {
                thread::sleep(time::Duration::from_millis(100));
                count.fetch_add(1, Ordering::Relaxed);
            })
        })
    });
    assert_eq!(count.into_inner(), 3 * 7);
}

#[test]
fn spawn_broadcast_mutual_sleepy() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let pool1 = Arc::new(ThreadPoolBuilder::new().num_threads(3).build().unwrap());
    let pool2 = ThreadPoolBuilder::new().num_threads(7).build().unwrap();
    pool1.spawn({
        let pool1 = Arc::clone(&pool1);
        move || {
            thread::sleep(time::Duration::from_secs(1));
            pool2.spawn_broadcast(move |_| {
                let tx = tx.clone();
                thread::sleep(time::Duration::from_secs(1));
                pool1.spawn_broadcast(move |_| {
                    thread::sleep(time::Duration::from_millis(100));
                    tx.send(()).unwrap();
                })
            })
        }
    });
    assert_eq!(rx.into_iter().count(), 3 * 7);
}

#[test]
fn broadcast_panic_one() {
    let count = AtomicUsize::new(0);
    let pool = ThreadPoolBuilder::new().num_threads(7).build().unwrap();
    let result = crate::unwind::halt_unwinding(|| {
        pool.broadcast(|ctx| {
            count.fetch_add(1, Ordering::Relaxed);
            if ctx.index() == 3 {
                panic!("Hello, world!");
            }
        })
    });
    assert_eq!(count.into_inner(), 7);
    assert!(result.is_err(), "broadcast panic should propagate!");
}

#[test]
fn spawn_broadcast_panic_one() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let (panic_tx, panic_rx) = crossbeam_channel::unbounded();
    let pool = ThreadPoolBuilder::new()
        .num_threads(7)
        .panic_handler(move |e| panic_tx.send(e).unwrap())
        .build()
        .unwrap();
    pool.spawn_broadcast(move |ctx| {
        tx.send(()).unwrap();
        if ctx.index() == 3 {
            panic!("Hello, world!");
        }
    });
    drop(pool); // including panic_tx
    assert_eq!(rx.into_iter().count(), 7);
    assert_eq!(panic_rx.into_iter().count(), 1);
}

#[test]
fn broadcast_panic_many() {
    let count = AtomicUsize::new(0);
    let pool = ThreadPoolBuilder::new().num_threads(7).build().unwrap();
    let result = crate::unwind::halt_unwinding(|| {
        pool.broadcast(|ctx| {
            count.fetch_add(1, Ordering::Relaxed);
            if ctx.index() % 2 == 0 {
                panic!("Hello, world!");
            }
        })
    });
    assert_eq!(count.into_inner(), 7);
    assert!(result.is_err(), "broadcast panic should propagate!");
}

#[test]
fn spawn_broadcast_panic_many() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let (panic_tx, panic_rx) = crossbeam_channel::unbounded();
    let pool = ThreadPoolBuilder::new()
        .num_threads(7)
        .panic_handler(move |e| panic_tx.send(e).unwrap())
        .build()
        .unwrap();
    pool.spawn_broadcast(move |ctx| {
        tx.send(()).unwrap();
        if ctx.index() % 2 == 0 {
            panic!("Hello, world!");
        }
    });
    drop(pool); // including panic_tx
    assert_eq!(rx.into_iter().count(), 7);
    assert_eq!(panic_rx.into_iter().count(), 4);
}
