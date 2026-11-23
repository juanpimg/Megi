use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap};

use super::TimerEntry;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TimerHeapItem {
    due_ms: u64,
    id: u64,
}

impl Ord for TimerHeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.due_ms
            .cmp(&other.due_ms)
            .then_with(|| self.id.cmp(&other.id))
    }
}

impl PartialOrd for TimerHeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug)]
struct LiveTimer {
    due_ms: u64,
    interval_ms: Option<u64>,
    animation: bool,
}

/// Min-heap timer queue ordered by `(due_ms, id)` with O(1) deferred cancellation (#2180).
#[derive(Clone, Debug, Default)]
pub(crate) struct TimerQueue {
    heap: BinaryHeap<Reverse<TimerHeapItem>>,
    live: HashMap<u64, LiveTimer>,
    animation_count: usize,
}

impl TimerQueue {
    /// Number of active (non-cancelled) timers.
    pub(crate) fn len(&self) -> usize {
        self.live.len()
    }

    /// Number of active `requestAnimationFrame` callbacks.
    pub(crate) fn animation_len(&self) -> usize {
        self.animation_count
    }

    /// Enqueues a newly scheduled timer in O(log n) amortized time.
    pub(crate) fn push(&mut self, entry: TimerEntry) {
        let is_new = self
            .live
            .insert(
                entry.id,
                LiveTimer {
                    due_ms: entry.due_ms,
                    interval_ms: entry.interval_ms,
                    animation: entry.animation,
                },
            )
            .is_none();
        if is_new && entry.animation {
            self.animation_count += 1;
        }
        self.heap.push(Reverse(TimerHeapItem {
            due_ms: entry.due_ms,
            id: entry.id,
        }));
        if self.heap.len() > self.live.len().saturating_mul(2).max(64) {
            self.compact();
        }
    }

    /// Cancels `id` if it belongs to the requested timer family (`animation`).
    pub(crate) fn cancel(&mut self, id: u64, animation: bool) -> bool {
        let matches = self
            .live
            .get(&id)
            .is_some_and(|meta| meta.animation == animation);
        if !matches {
            return false;
        }
        self.live.remove(&id);
        if animation {
            self.animation_count = self.animation_count.saturating_sub(1);
        }
        true
    }

    /// Pops the earliest live timer due at or before `effective_ms`.
    ///
    /// Repeating intervals are immediately rescheduled at `due_ms + interval_ms`.
    pub(crate) fn pop_due(&mut self, effective_ms: u64) -> Option<TimerEntry> {
        loop {
            let item = self.heap.peek()?.0;
            if item.due_ms > effective_ms {
                return None;
            }
            self.heap.pop();
            let Some(meta) = self.live.get_mut(&item.id) else {
                continue;
            };
            if meta.due_ms != item.due_ms {
                continue;
            }
            let entry = TimerEntry {
                id: item.id,
                due_ms: item.due_ms,
                interval_ms: meta.interval_ms,
                animation: meta.animation,
            };
            if let Some(interval) = meta.interval_ms {
                let next_due = item.due_ms.saturating_add(interval);
                meta.due_ms = next_due;
                self.heap.push(Reverse(TimerHeapItem {
                    due_ms: next_due,
                    id: item.id,
                }));
            } else {
                let animation = meta.animation;
                self.live.remove(&item.id);
                if animation {
                    self.animation_count = self.animation_count.saturating_sub(1);
                }
            }
            return Some(entry);
        }
    }

    fn compact(&mut self) {
        self.heap.retain(|Reverse(item)| {
            self.live
                .get(&item.id)
                .is_some_and(|meta| meta.due_ms == item.due_ms)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_by_due_then_id_and_handles_deferred_cancel() {
        let mut q = TimerQueue::default();
        q.push(TimerEntry {
            id: 1,
            due_ms: 20,
            interval_ms: None,
            animation: false,
        });
        q.push(TimerEntry {
            id: 2,
            due_ms: 10,
            interval_ms: None,
            animation: false,
        });
        q.push(TimerEntry {
            id: 3,
            due_ms: 10,
            interval_ms: None,
            animation: true,
        });
        assert_eq!(q.len(), 3);
        assert_eq!(q.animation_len(), 1);
        assert!(!q.cancel(3, false));
        assert!(q.cancel(2, false));
        assert_eq!(q.len(), 2);
        assert_eq!(q.pop_due(9).map(|e| e.id), None);
        assert_eq!(q.pop_due(20).map(|e| e.id), Some(3));
        assert_eq!(q.animation_len(), 0);
        assert_eq!(q.pop_due(20).map(|e| e.id), Some(1));
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn reschedules_intervals_and_compacts_cancelled_tombstones() {
        let mut q = TimerQueue::default();
        q.push(TimerEntry {
            id: 10,
            due_ms: 5,
            interval_ms: Some(10),
            animation: false,
        });
        assert_eq!(q.pop_due(5).map(|e| e.due_ms), Some(5));
        assert_eq!(q.pop_due(14).map(|e| e.due_ms), None);
        assert_eq!(q.pop_due(15).map(|e| e.due_ms), Some(15));
        assert!(q.cancel(10, false));
        assert!(q.pop_due(100).is_none());

        for id in 100..600 {
            q.push(TimerEntry {
                id,
                due_ms: id,
                interval_ms: None,
                animation: false,
            });
            assert!(q.cancel(id, false));
        }
        assert_eq!(q.len(), 0);
        assert!(q.heap.len() <= 64);
    }
}
