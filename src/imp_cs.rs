use core::{
    cell::UnsafeCell,
    panic::{RefUnwindSafe, UnwindSafe},
};

use portable_atomic::{AtomicBool, Ordering};

use crate::unsync;

pub(crate) struct OnceCell<T> {
    initialized: AtomicBool,
    // Initialization is serialized by a critical section. After publication,
    // shared access never mutates the value, so readers need not acquire one.
    value: UnsafeCell<unsync::OnceCell<T>>,
}

// Why do we need `T: Send`?
// Thread A creates a `OnceCell` and shares it with
// scoped thread B, which fills the cell, which is
// then destroyed by A. That is, destructor observes
// a sent value.
unsafe impl<T: Sync + Send> Sync for OnceCell<T> {}
unsafe impl<T: Send> Send for OnceCell<T> {}

impl<T: RefUnwindSafe + UnwindSafe> RefUnwindSafe for OnceCell<T> {}
impl<T: UnwindSafe> UnwindSafe for OnceCell<T> {}

impl<T> OnceCell<T> {
    pub(crate) const fn new() -> OnceCell<T> {
        OnceCell {
            initialized: AtomicBool::new(false),
            value: UnsafeCell::new(unsync::OnceCell::new()),
        }
    }

    pub(crate) const fn with_value(value: T) -> OnceCell<T> {
        OnceCell {
            initialized: AtomicBool::new(true),
            value: UnsafeCell::new(unsync::OnceCell::with_value(value)),
        }
    }

    #[inline]
    pub(crate) fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    #[cold]
    pub(crate) fn initialize<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        critical_section::with(|_| {
            // SAFETY: The critical section serializes initialization. Shared
            // readers only access the cell after its value becomes immutable.
            let cell = unsafe { &*self.value.get() };
            cell.get_or_try_init(f).map(|_| {
                self.initialized.store(true, Ordering::Release);
            })
        })
    }

    /// Get the reference to the underlying value, without checking if the cell
    /// is initialized.
    ///
    /// # Safety
    ///
    /// Caller must ensure that the cell is in initialized state, and that
    /// the contents are acquired by (synchronized to) this thread.
    pub(crate) unsafe fn get_unchecked(&self) -> &T {
        debug_assert!(self.is_initialized());
        // SAFETY: The caller acquired initialization, and the cell is never
        // changed through shared access after publication. The reference cannot
        // outlive self; mutable access and extraction require exclusive ownership.
        (&*self.value.get()).get().unwrap_unchecked()
    }

    #[inline]
    pub(crate) fn get_mut(&mut self) -> Option<&mut T> {
        self.value.get_mut().get_mut()
    }

    #[inline]
    pub(crate) fn into_inner(self) -> Option<T> {
        self.value.into_inner().into_inner()
    }
}
