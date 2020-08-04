#![no_std]
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, spin_loop_hint};
use core::sync::atomic::Ordering::{Acquire, Release};

/// An Atomic Cell game for up to two players.
#[derive(Debug)]
pub struct PingPongCell<T> {
    state: AtomicUsize,
    value: UnsafeCell<Option<T>>,
}

const WAITING: usize = 0;
const WORKING: usize = 0b01;

impl<T: Send> PingPongCell<T> {

    /// Create a new PingPongCell
    pub fn new(value: Option<T>) -> Self {
        PingPongCell {
            state: AtomicUsize::new(WAITING),
            value: UnsafeCell::new(value),
        }
    }

    /// If there is a value currently inside, take it.
    pub fn take(&self) -> Option<T> {
        loop {
            if let WAITING = self.state.compare_and_swap(WAITING, WORKING, Acquire) {
                return unsafe {
                    let val = (*self.value.get()).take();
                    self.state.store(WORKING, Release);
                    val
                }
            } else {
                spin_loop_hint();
            }
        }
    }

    /// Replace the value inside unconditionally.
    pub fn put(&self, value: T) {
        loop {
            if let WAITING = self.state.compare_and_swap(WAITING, WORKING, Acquire) {
                unsafe {
                    *self.value.get() = Some(value);
                    self.state.store(WAITING, Release);
                    return
                }
            } else {
                spin_loop_hint();
            }
        }
    }

    /// Puts a value inside only if there is not one currently.
    pub fn put_if_empty(&self, value: T) -> Result<(), T> {
        loop {
            if let WAITING = self.state.compare_and_swap(WAITING, WORKING, Acquire) {
                return unsafe {
                    let inner = self.value.get();
                    if (*inner).is_some() {
                        Err(value)
                    } else {
                        *inner = Some(value);
                        self.state.store(WAITING, Release);
                        Ok(())
                    }
                }
            } else {
                spin_loop_hint();
            }
        }
    }
}

impl<T: Clone + Send> PingPongCell<T> {
    /// Clones the contents, if any.
    pub fn clone_inner(&self) -> Option<T> {
        loop {
            if let WAITING = self.state.compare_and_swap(WAITING, WORKING, Acquire) {
                return unsafe {
                    let val = (*self.value.get()).clone();
                    self.state.store(WAITING, Release);
                    val
                }
            } else {
                spin_loop_hint();
            }
        }
    }
}

impl<T: Eq + Send> PingPongCell<T> {
    /// A single CAS operation
    pub fn compare_and_swap(&self, expected: &T, new: T) -> Result<(), T> {
        loop {
            if let WAITING = self.state.compare_and_swap(WAITING, WORKING, Acquire) {
                return unsafe {
                    let inner = self.value.get();
                    if let Some(val) = &*inner {
                        let ret = if val == expected {
                            *inner = Some(new);
                            Ok(())
                        } else {
                            Err(new)
                        };
                        self.state.store(WAITING, Release);
                        ret
                    } else {
                        self.state.store(WAITING, Release);
                        Err(new)
                    }
                }
            } else {
                spin_loop_hint();
            }
        }
    }
}

impl<T: Clone + Eq + Send> PingPongCell<T> {
    /// A single CAS operation
    pub fn compare_swap_clone(&self, expected: &T, new: T) -> Result<(), (T, Option<T>)> {
        loop {
            if let WAITING = self.state.compare_and_swap(WAITING, WORKING, Acquire) {
                return unsafe {
                    let inner = self.value.get();
                    if let Some(val) = &*inner {
                        let ret = if val == expected {
                            *inner = Some(new);
                            Ok(())
                        } else {
                            Err((new, Some(val.clone())))
                        };
                        self.state.store(WAITING, Release);
                        ret
                    } else {
                        self.state.store(WAITING, Release);
                        Err((new, None))
                    }
                }
            } else {
                spin_loop_hint();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // #[test]
    // fn it_works() {
    //     assert_eq!(2 + 2, 4);
    // }
}
