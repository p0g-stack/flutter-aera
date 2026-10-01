//! The platform task runner: Flutter posts tasks with a target time, the
//! platform thread runs them when due. Same shape as flutter-pi's (tasks on
//! its main loop, woken by an eventfd) and GTK's (tasks as GLib timeouts on
//! the main context).

use std::collections::BinaryHeap;
use std::cmp::Reverse;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::Mutex;
use std::thread::ThreadId;

use crate::ffi::FlutterTask;

struct Pending {
    target_ns: u64,
    order: u64,
    task: FlutterTask,
}

impl PartialEq for Pending {
    fn eq(&self, other: &Self) -> bool {
        (self.target_ns, self.order) == (other.target_ns, other.order)
    }
}
impl Eq for Pending {}
impl PartialOrd for Pending {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Pending {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.target_ns, self.order).cmp(&(other.target_ns, other.order))
    }
}

struct Queue {
    heap: BinaryHeap<Reverse<Pending>>,
    order: u64,
}

pub struct TaskRunner {
    thread: ThreadId,
    queue: Mutex<Queue>,
    wake: OwnedFd,
}

// SAFETY: FlutterTask is an opaque runner pointer plus an id; the engine
// allows running it on the runner's thread, which is the only place we do.
unsafe impl Send for TaskRunner {}
unsafe impl Sync for TaskRunner {}

impl TaskRunner {
    /// A runner for the calling thread.
    pub fn new() -> io::Result<TaskRunner> {
        // SAFETY: plain eventfd creation.
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(TaskRunner {
            thread: std::thread::current().id(),
            queue: Mutex::new(Queue { heap: BinaryHeap::new(), order: 0 }),
            // SAFETY: fd was just created and is owned by nothing else.
            wake: unsafe { OwnedFd::from_raw_fd(fd) },
        })
    }

    pub fn runs_on_current_thread(&self) -> bool {
        std::thread::current().id() == self.thread
    }

    pub fn post(&self, task: FlutterTask, target_ns: u64) {
        let mut q = self.queue.lock().unwrap();
        q.order += 1;
        let order = q.order;
        q.heap.push(Reverse(Pending { target_ns, order, task }));
        drop(q);
        self.wake();
    }

    /// Wakes the platform loop (from any thread).
    pub fn wake(&self) {
        let one: u64 = 1;
        // SAFETY: writes 8 bytes from a valid u64 to our eventfd.
        unsafe { libc::write(self.wake.as_raw_fd(), (&one as *const u64).cast(), 8) };
    }

    /// Clears pending wakes; call after poll reports the fd readable.
    pub fn drain_wake(&self) {
        let mut value: u64 = 0;
        // SAFETY: reads 8 bytes into a valid u64; EAGAIN when empty is fine.
        unsafe { libc::read(self.wake.as_raw_fd(), (&mut value as *mut u64).cast(), 8) };
    }

    pub fn wake_fd(&self) -> RawFd {
        self.wake.as_raw_fd()
    }

    pub fn next_target(&self) -> Option<u64> {
        self.queue.lock().unwrap().heap.peek().map(|p| p.0.target_ns)
    }

    /// Removes and returns every task due at `now_ns`, in order.
    pub fn take_due(&self, now_ns: u64) -> Vec<FlutterTask> {
        let mut q = self.queue.lock().unwrap();
        let mut due = vec![];
        while q.heap.peek().is_some_and(|p| p.0.target_ns <= now_ns) {
            due.push(q.heap.pop().unwrap().0.task);
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(n: u64) -> FlutterTask {
        FlutterTask { runner: std::ptr::null_mut(), task: n }
    }

    #[test]
    fn runs_in_target_then_post_order() {
        let r = TaskRunner::new().unwrap();
        r.post(task(1), 30);
        r.post(task(2), 10);
        r.post(task(3), 10);
        assert_eq!(r.next_target(), Some(10));
        let ids: Vec<u64> = r.take_due(20).iter().map(|t| t.task).collect();
        assert_eq!(ids, vec![2, 3]);
        assert_eq!(r.take_due(29).len(), 0);
        assert_eq!(r.take_due(30)[0].task, 1);
        assert!(r.runs_on_current_thread());
    }
}
