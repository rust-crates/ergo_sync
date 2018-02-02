//! make creating and synchronizing threads ergonomic, therefore fun!
//!
//! This is the synchronization library as part of the `ergo` crates ecosystem. It contains useful
//! types, traits and functions for spawning threads and synchronizing them. It is named `sync`
//! because of `std::sync` and because it is _not_ async, which is/will be a spearate part of the
//! ergo ecocystem.
//!
//! The crates that are wraped/exported are:
//!
//! - [`rayon`](https://github.com/rayon-rs/rayon): Rayon: A data parallelism library for Rust
//! - [`chan`](https://github.com/BurntSushi/chan): Multi-producer, multi-consumer concurrent
//!   channel for Rust.
//! - [`taken`](https://github.com/vitiral/taken): macros for taking ownership
//!
//! Consider supporting their development individually and starring them on github.
//!
//! > **This crate is a WIP. More docs will be added in the future.**
//!
//! # Examples
//!
//! ## Example: most of the features together
//!
//! ```rust
//! #[macro_use] extern crate ergo_sync;
//! use ergo_sync::*;
//!
//! # fn main() {
//! let val = 42;
//!
//! // rendezvous channel
//! let (send, recv) = chan::bounded(0);
//!
//! // The consumer must be spawned in its own thread or we risk
//! // deadlock. Do NOT put the consumer in the threadpool, as
//! // threadpools do not guarantee >1 threads running at a time.
//! let consumer = spawn(|| -> u64 {
//!     take!(recv); // same as `let recv = recv`
//!     recv.iter().sum()
//! });
//!
//! // spawn and join N number threads
//! pool_join!{
//!     {
//!         let s = send.clone();
//!         // do some expensive function
//!         s.send(42_u64.pow(4)).unwrap();
//!     },
//!     {
//!         // Each function can also use rayon's traits to do iteration in parallel.
//!         take!(=send as s); // same as `let s = send.clone()`
//!         (0..1000_u64).into_par_iter().for_each(|n| {
//!             s.send(n * 42).unwrap();
//!         });
//!     },
//!     {
//!         take!(=send as s, &val);
//!         s.send(expensive_fn(val)).unwrap();
//!     },
//! };
//!
//! drop(send); // the channel must be dropped for iterator to stop.
//! assert_eq!(24_094_896, consumer.finish());
//! # }
//!
//! /// Really expensive function
//! fn expensive_fn(v: &u32) -> u64 {
//!     println!("Doing expensive thing");
//!     sleep_ms(300);
//!     *v as u64 * 100
//! }
//! ```
//!
//! # Example: multiple producers and multiple consumers using channels
//!
//! This example is addapted from the [chan docs].
//!
//! [chan docs]: https://docs.rs/chan/0.1.20/chan/#example-multiple-producers-and-multiple-consumers
//!
//! ```
//! #[macro_use] extern crate ergo_sync;
//! use ergo_sync::*;
//!
//! # fn main() {
//! let receiving = {
//!     // This scope prevents us from forgetting to drop the sending channel,
//!     // as both `send` and `recv` are dropped at the end of the scope.
//!     let (send, recv) = chan::bounded(0);
//!
//!     // Kick off the receiving threads.
//!     //
//!     // Note that these do _not_ run in the rayon thread pool,
//!     // they are simple OS level threads from `std::thread::spawn`.
//!     let mut receiving = Vec::with_capacity(4);
//!     for _ in 0..4 {
//!         take!(=recv as r); // take a clone of `recv`
//!         receiving.push(spawn(|| {
//!             for letter in r {
//!                 println!("Received letter: {}", letter);
//!             }
//!         }));
//!     }
//!
//!     // Send values in parallel using the rayon thread pool.
//!     let mut chars: Vec<_> = "A man, a plan, a canal - Panama!"
//!         .chars()
//!         .collect();
//!     chars.into_par_iter().map(|letter| {
//!         take!(=send as s); // take a clone of `send`
//!         for _ in 0..10 {
//!             s.send(letter).unwrap();
//!         }
//!     });
//!
//!     // You must wait for the threads _outside_ of this scope, else you
//!     // will get deadlock.
//!     //
//!     // You could also call `drop(send)`, in which case you would not
//!     // need the scope at all. However, if you had more than one sending
//!     // channel you would also need to remember to drop _that_, etc etc.
//!     receiving
//! };
//!
//! // Wait until all threads have finished before exiting.
//! //
//! // Alternatively we could have used `chan::WaitGroup` in the
//! // receiving threads to keep track of when threads finished,
//! // however we would have to be diligent to make sure we don't
//! // forget to call `wg.add/done` at the appropriate times.
//! //
//! // `chan::WaitGroup` scales much better... but how often
//! // are you tracking more than 100 threads?
//! for r in receiving {
//!     r.finish();
//! }
//! # }
//! ```

#[allow(unused_imports)]
#[macro_use(take)]
extern crate taken;
pub extern crate crossbeam_channel;
pub extern crate rayon;
pub extern crate std_prelude;

pub mod chan {
    pub use crossbeam_channel::*;
}

// -------- std_prelude exports --------
// Types
pub use std_prelude::{Arc, Duration, Mutex};
// Atomics
pub use std_prelude::{AtomicBool, AtomicIsize, AtomicOrdering, AtomicUsize, ATOMIC_USIZE_INIT};
// Functions
pub use std_prelude::{sleep, spawn};

// -------- macro exports--------
#[allow(unused_imports)]
#[doc(hidden)]
pub mod reexports {
    // hack to rexport macros
    #[doc(hidden)] pub use chan::*;
    #[doc(hidden)] pub use taken::*;
}
pub use reexports::*;

// -------- other exports --------
pub use rayon::prelude::*;


use std_prelude::*;

/// Convinience trait mimicking `std::thread::JoinHandle` with better ergonomics.
pub trait FinishHandle<T>
where
    T: Send + 'static,
{
    fn finish(self) -> T;
}

impl<T: Send + 'static> FinishHandle<T> for ::std::thread::JoinHandle<T> {
    /// Finishes the thread, returning the value.
    ///
    /// This is the same as `JoinHandle::join()` except the unwrap is automatic.
    ///
    /// # Panics
    /// Panics if the thread is poisoned (if a panic happened inside the thread).
    ///
    /// # Examples
    /// ```rust
    /// # extern crate ergo_sync;
    /// # use ergo_sync::*;
    /// # fn main() {
    /// // sleep for half a second
    /// let th = spawn(|| sleep_ms(100));
    /// th.finish(); // as opposed to `th.join().unwrap()`
    /// # }
    /// ```
    fn finish(self) -> T {
        self.join()
            .expect("finish failed to join, thread is poisoned")
    }
}

/// Spawn multiple _scoped_ threads and then join them. These are run using the _current scope_ in
/// the rayon threadpool and are not necessarily guaranteed to run in parallel.
///
/// The fact that they are scoped means that you can reference variables from the current stack,
/// since the thread is guaranteed to terminate after the `pool_join!` statement is complete.
///
/// This is slower than using _either_ `rayon::join` or rayon's parallel iterators. It also
/// requires heap allocations. See the rayon documentation for [`scope`](../rayon/fn.scope.html)
/// for more details and alternatives.
///
/// Although it is less efficient than other APIs exposed by rayon, it can be unergonomic to use
/// rayon directly when you want to run more than 2 workloads in parallel. This _is_ ergonomic and
/// for most use cases is _efficient enough_.
///
/// # Examples
/// ```
/// #[macro_use] extern crate ergo_sync;
/// use ergo_sync::*;
///
/// # fn main() {
/// let (send, recv) = chan::bounded(0); // rendezvous channel
///
/// // The consumer must be spawned in a thread or we risk deadlock
/// // Do NOT put the consumer in the threadpool, as it does not
/// // guarantee >1 thread running at a time.
/// let consumer = spawn(move|| {
///     let recv = recv;
///     recv.iter().sum()
/// });
///
/// pool_join!{
///     {
///         let send = send.clone();
///         send.send(4).unwrap();
///     },
///     {
///         take!(=send); // let send = send.clone()
///         send.send(12).unwrap();
///     },
///     {
///         take!(=send as s); // let s = send.clone()
///         s.send(26).unwrap();
///     },
/// };
///
/// drop(send); // the channel must be dropped for iterator to stop.
/// assert_eq!(42, consumer.finish());
/// # }
/// ```
#[macro_export]
macro_rules! pool_join {
    ( $( $thread:expr ),* $(,)* ) => {
        rayon::scope(|s| {
            $(
                s.spawn(|_| $thread);
            )*
        });
    };
}

/// Just sleep for a certain number of milliseconds.
///
/// Equivalent of `sleep(Duration::from_millis(millis))`
///
/// This function exists in `std::thread`, so it created here instead.
///
/// # Examples
/// ```rust
/// # extern crate ergo_sync;
/// # use ergo_sync::*;
/// # fn main() {
/// // sleep for half a second
/// sleep_ms(500);
/// # }
/// ```
#[inline(always)]
pub fn sleep_ms(millis: u64) {
    sleep(Duration::from_millis(millis))
}
