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

unsafe impl<T: Send> Send for PingPongCell<T> {}
unsafe impl<T: Sync> Sync for PingPongCell<T> {}

const WAITING: usize = 0;
const WORKING: usize = 0b01;

impl<T> PingPongCell<T> {

    /// Create a new PingPongCell
    pub fn new(value: Option<T>) -> Self {
        PingPongCell {
            state: AtomicUsize::new(WAITING),
            value: UnsafeCell::new(value),
        }
    }

    /// If there is a value currently inside, take it.
    pub fn take(&self) -> Option<T> {
        self.transact(|state| state.take() )
    }

    /// Replace the value inside unconditionally.
    pub fn put(&self, value: T) {
        self.transact(|state| { *state = Some(value); })
    }

    /// Puts a value inside only if there is not one currently.
    pub fn put_if_empty(&self, value: T) -> Result<(), T> {
        self.transact(|state| {
            if state.is_some() {
                Err(value)
            } else {
                *state = Some(value);
                Ok(())
            }
        })
    }

    /// Runs a closure with a mutable reference to the state
    pub fn transact<F, R>(&self, fun: F) -> R
    where F: FnOnce(&mut Option<T>) -> R {
        loop {
            if let WAITING = self.state.compare_and_swap(WAITING, WORKING, Acquire) {
                return unsafe {
                    let ret = fun(&mut *self.value.get());
                    self.state.store(WAITING, Release);
                    ret
                }
            } else {
                spin_loop_hint();
            }
        }
    }
}

impl<T: Clone> PingPongCell<T> {

    /// `put_empty()`, but clones the current contents on failure
    pub fn put_empty_clone(&self, value: T) -> Result<(), (T, T)> {
        self.transact(|state| {
            if let Some(ref old) = state {
                Err((value, old.clone()))
            } else {
                *state = Some(value);
                Ok(())
            }
        })
    }

    /// Clones the contents, if any.
    pub fn clone_inner(&self) -> Option<T> {
        self.transact(|state| {
            state.clone()
        })
    }
}

impl<T: Eq> PingPongCell<T> {
    /// A single CAS operation
    pub fn compare_and_swap(&self, expected: &T, new: T) -> Result<(), T> {
        self.transact(|state| {
            if let Some(val) = state {
                if val == expected {
                    *state = Some(new);
                    return Ok(())
                }
            }
            Err(new)
        })
    }
}

impl<T: Clone + Eq> PingPongCell<T> {

    /// A single CAS operation, cloning the current value on failure
    pub fn compare_swap_clone(
        &self,
        expected: &T,
        new: T
    ) -> Result<(), (T, Option<T>)> {
        self.transact(|state| {
            if let Some(val) = state {
                if val == expected {
                    *state = Some(new);
                    Ok(())
                } else {
                    Err((new, Some(val.clone())))
                }
            } else {
                Err((new, None))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    // #[test]
    // fn it_works() {
    //     assert_eq!(2 + 2, 4);
    // }
}
