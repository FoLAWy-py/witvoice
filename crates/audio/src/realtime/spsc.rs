//! Single producer/consumer ownership. Storage is allocated before split.
use std::{
    cell::{Cell, UnsafeCell},
    marker::PhantomData,
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingError {
    Capacity,
    Full,
}

pub struct Ring<T: Copy + Send> {
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
    head: AtomicUsize,
    tail: AtomicUsize,
}
// SAFETY: split requires exclusive &mut Ring, so there is exactly one producer
// and consumer. Neither endpoint is Clone or Sync. Producer owns a free slot
// until release-publishing head; consumer owns a published slot until release-
// publishing tail. Acquire of the other cursor prevents reuse/read races.
// T: Copy has no destructor and references to slots never escape. Ring storage
// outlives both borrowed endpoints and cannot be moved/dropped during callbacks.
unsafe impl<T: Copy + Send> Sync for Ring<T> {}
impl<T: Copy + Send> Ring<T> {
    pub fn new(capacity: usize) -> Result<Self, RingError> {
        if !(1..=64).contains(&capacity) {
            return Err(RingError::Capacity);
        }
        Ok(Self {
            slots: (0..capacity)
                .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
                .collect(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        })
    }
    pub fn split(&mut self) -> (Producer<'_, T>, Consumer<'_, T>) {
        let ring = &*self;
        (
            Producer {
                ring,
                _single: PhantomData,
            },
            Consumer {
                ring,
                _single: PhantomData,
            },
        )
    }
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }
    fn distance(&self, head: usize, tail: usize) -> usize {
        (head + 2 * self.capacity() - tail) % (2 * self.capacity())
    }
    fn next(&self, cursor: usize) -> usize {
        (cursor + 1) % (2 * self.capacity())
    }
}
pub struct Producer<'a, T: Copy + Send> {
    ring: &'a Ring<T>,
    _single: PhantomData<Cell<()>>,
}
pub struct Consumer<'a, T: Copy + Send> {
    ring: &'a Ring<T>,
    _single: PhantomData<Cell<()>>,
}
impl<T: Copy + Send> Producer<'_, T> {
    pub fn try_push(&mut self, value: T) -> Result<(), RingError> {
        let head = self.ring.head.load(Ordering::Relaxed);
        let tail = self.ring.tail.load(Ordering::Acquire);
        if self.ring.distance(head, tail) == self.ring.capacity() {
            return Err(RingError::Full);
        }
        unsafe {
            (*self.ring.slots[head % self.ring.capacity()].get()).write(value);
        }
        self.ring
            .head
            .store(self.ring.next(head), Ordering::Release);
        Ok(())
    }
    pub fn capacity(&self) -> usize {
        self.ring.capacity()
    }
}
impl<T: Copy + Send> Consumer<'_, T> {
    pub fn try_pop(&mut self) -> Option<T> {
        let tail = self.ring.tail.load(Ordering::Relaxed);
        if self.ring.head.load(Ordering::Acquire) == tail {
            return None;
        }
        let value =
            unsafe { (*self.ring.slots[tail % self.ring.capacity()].get()).assume_init_read() };
        self.ring
            .tail
            .store(self.ring.next(tail), Ordering::Release);
        Some(value)
    }
    pub fn queued(&self) -> usize {
        self.ring.distance(
            self.ring.head.load(Ordering::Acquire),
            self.ring.tail.load(Ordering::Relaxed),
        )
    }
    pub fn capacity(&self) -> usize {
        self.ring.capacity()
    }
}
