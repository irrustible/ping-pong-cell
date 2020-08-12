#![no_std]
use core::cell::UnsafeCell;
use core::sync::atomic::Ordering::{Acquire, Release};
use core::sync::atomic::{spin_loop_hint, AtomicBool};

/// An Atomic Cell game for up to two players.
#[derive(Debug)]
pub struct PingPongCell<T> {
    is_working: AtomicBool,
    value: UnsafeCell<Option<T>>,
}

unsafe impl<T: Send> Send for PingPongCell<T> {}
unsafe impl<T: Sync> Sync for PingPongCell<T> {}

impl<T> PingPongCell<T> {
    /// Create a new PingPongCell
    pub fn new(value: Option<T>) -> Self {
        PingPongCell {
            is_working: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    /// If there is a value currently inside, take it.
    #[inline]
    pub fn take(&self) -> Option<T> {
        self.transact(|state| state.take())
    }

    /// Replace the value inside unconditionally.
    #[inline]
    pub fn put(&self, value: T) {
        self.transact(|state| *state = Some(value));
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
    where
        F: FnOnce(&mut Option<T>) -> R,
    {
        while self.is_working.compare_and_swap(false, true, Acquire) {
            spin_loop_hint();
        }
        let ret = unsafe { fun(&mut *self.value.get()) };
        self.is_working.store(false, Release);
        ret
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
        self.transact(|state| state.clone())
    }
}

impl<T: Eq> PingPongCell<T> {
    /// A single CAS operation
    pub fn compare_and_swap(&self, expected: &T, new: T) -> Result<(), T> {
        self.transact(|state| match state {
            Some(ref mut val) if val == expected => {
                *val = new;
                Ok(())
            }
            _ => Err(new),
        })
    }
}

impl<T: Clone + Eq> PingPongCell<T> {
    /// A single CAS operation, cloning the current value on failure
    pub fn compare_swap_clone(&self, expected: &T, new: T) -> Result<(), (T, Option<T>)> {
        self.transact(|state| match state {
            Some(val) if val == expected => {
                *state = Some(new);
                Ok(())
            }
            Some(val) => Err((new, Some(val.clone()))),
            None => Err((new, None)),
        })
    }
}
