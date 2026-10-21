//! `std/channel` (RFC 0002 §2.9, plan `web-apis.md` phase 10), the twin of the JS runtime's
//! `channel_*` functions: a bounded FIFO inside the instance. A channel is an ordinary
//! reference-counted object (tag 19, ABI §3.2) that `Sender` and `Receiver` both are; it owns
//! its queued items, and the trampoline releases the words a waiting code keeps in its frame
//! if its task unwinds. `channel_with`'s finalizer closes and empties it on every exit of the
//! body, which also breaks a cycle through an item that holds the channel's own end.
//!
//! A task that cannot go on waits in the channel's senders or receivers (task id and epoch,
//! `Rt::wait_here`); every change that may let one go on (an item queued, an item taken, the
//! close) wakes every waiter of the other kind in the order they waited, at the back of the run
//! queue, and each tries again (JS: their promises resolve in that order), so a wake-up lost to
//! a cancelled waiter cannot strand another. A waiter cancelled since it waited is skipped by
//! its epoch.

use crate::exec::step;
use crate::scratch::Stack;
use crate::{Defect, Res, Rt, tag};

const CAP: u32 = 8;
const CLOSED: u32 = 12;
/// The index of the oldest item in `ITEMS`.
const HEAD: u32 = 16;
const ITEMS: u32 = 20;
/// Waiting senders and receivers: (task id, epoch) pairs.
const SENDERS: u32 = 32;
const RECEIVERS: u32 = 44;
const SIZE: u32 = 56;

/// `channel_try_send`'s and `channel_send`'s answers (`std/channel`).
const QUEUED: i64 = 0;
const FULL: i64 = 1;
const SHUT: i64 = 2;

impl Rt {
    /// A fresh open channel of `capacity` items (`std/channel` requires it positive; a
    /// capacity past `u32::MAX` is as large as memory allows).
    pub fn channel_new(&mut self, capacity: i64) -> Res<u32> {
        let ch = self.new_object(tag::CHANNEL, SIZE)?;
        let cap = capacity.clamp(1, i64::from(u32::MAX)) as u32;
        self.heap.mem.set_u32(ch + CAP, cap);
        self.heap.mem.set_u32(ch + CLOSED, 0);
        self.heap.mem.set_u32(ch + HEAD, 0);
        for off in [ITEMS, SENDERS, RECEIVERS] {
            Stack::NONE.store(&mut self.heap.mem, ch + off);
        }
        Ok(ch)
    }

    fn queued(&self, ch: u32) -> u32 {
        let items = Stack::load(&self.heap.mem, ch + ITEMS);
        items.len - self.heap.mem.u32(ch + HEAD)
    }

    fn closed(&self, ch: u32) -> bool {
        self.heap.mem.u32(ch + CLOSED) != 0
    }

    /// Queue `v` (consumed) and wake the receivers.
    fn enqueue_item(&mut self, ch: u32, v: u32) -> Res<()> {
        let mut items = Stack::load(&self.heap.mem, ch + ITEMS);
        let r = self.push(&mut items, v);
        items.store(&mut self.heap.mem, ch + ITEMS);
        if r.is_err() {
            self.drop(v);
        }
        r?;
        self.wake_all(ch, RECEIVERS);
        Ok(())
    }

    /// Take the oldest item (owned) and wake the senders. The block is compacted once the
    /// taken prefix is half of it, so it holds about `capacity` words.
    fn dequeue_item(&mut self, ch: u32) -> u32 {
        let mut items = Stack::load(&self.heap.mem, ch + ITEMS);
        let head = self.heap.mem.u32(ch + HEAD);
        let v = self.stack_at(&items, head);
        let mut head = head + 1;
        if head == items.len {
            items.len = 0;
            head = 0;
        } else if head >= 32 && 2 * head >= items.len {
            for i in head..items.len {
                let w = self.stack_at(&items, i);
                self.stack_set(&items, i - head, w);
            }
            items.len -= head;
            head = 0;
        }
        items.store(&mut self.heap.mem, ch + ITEMS);
        self.heap.mem.set_u32(ch + HEAD, head);
        self.wake_all(ch, SENDERS);
        v
    }

    /// Wake every waiter in the list at `off`, in the order they waited, and empty it.
    fn wake_all(&mut self, ch: u32, off: u32) {
        let mut list = Stack::load(&self.heap.mem, ch + off);
        let n = list.len;
        list.len = 0;
        list.store(&mut self.heap.mem, ch + off);
        for i in (0..n).step_by(2) {
            let (id, epoch) = (self.stack_at(&list, i), self.stack_at(&list, i + 1));
            self.wake_waiter(id, epoch);
        }
    }

    /// Block the current task in the list at `off`; the caller returns `step::YIELD` and tries
    /// again when woken.
    fn wait_in(&mut self, ch: u32, off: u32) -> Res<u32> {
        if let Some((id, epoch)) = self.wait_here() {
            let mut list = Stack::load(&self.heap.mem, ch + off);
            let r = self
                .push(&mut list, id)
                .and_then(|_| self.push(&mut list, epoch));
            list.store(&mut self.heap.mem, ch + off);
            r?;
        }
        Ok(step::YIELD)
    }

    /// `channel_try_send(ch, v)`: 0 queued, 1 full, 2 closed; never waits. Consumes both.
    pub fn channel_try_send(&mut self, ch: u32, v: u32) -> Res<i64> {
        let r = if self.closed(ch) {
            self.drop(v);
            Ok(SHUT)
        } else if self.queued(ch) >= self.heap.mem.u32(ch + CAP) {
            self.drop(v);
            Ok(FULL)
        } else {
            self.enqueue_item(ch, v).map(|_| QUEUED)
        };
        self.drop(ch);
        r
    }

    /// `channel_send(ch, v)` with both in `frame`'s slots 0 and 1: 0 queued or 2 closed in the
    /// return register (both taken), or the task waits and this runs again when it is woken.
    pub fn channel_send(&mut self, frame: u32) -> Res<u32> {
        let ch = self.fslot(frame, 0) as u32;
        if !self.closed(ch) && self.queued(ch) >= self.heap.mem.u32(ch + CAP) {
            return self.wait_in(ch, SENDERS);
        }
        let v = self.take_fslot(frame, 1) as u32;
        let ch = self.take_fslot(frame, 0) as u32;
        let r = self.channel_try_send(ch, v)?;
        self.set_ret(r as u64);
        Ok(step::RETURN)
    }

    /// `channel_recv(ch)` with it in `frame`'s slot 0: `Some(item)`, or `None` once closed and
    /// drained, in the return register (the channel taken), or the task waits and this runs
    /// again when it is woken.
    pub fn channel_recv(&mut self, frame: u32) -> Res<u32> {
        let ch = self.fslot(frame, 0) as u32;
        let r = if self.queued(ch) > 0 {
            let v = self.dequeue_item(ch);
            self.some(v)?
        } else if self.closed(ch) {
            self.none()
        } else {
            return self.wait_in(ch, RECEIVERS);
        };
        let ch = self.take_fslot(frame, 0) as u32;
        self.drop(ch);
        self.set_ret(u64::from(r));
        Ok(step::RETURN)
    }

    /// `channel_close(ch)`: later sends fail, every waiter wakes; a second close does nothing.
    /// Consumes `ch`.
    pub fn channel_close(&mut self, ch: u32) {
        self.close_channel(ch);
        self.drop(ch);
    }

    fn close_channel(&mut self, ch: u32) {
        if self.closed(ch) {
            return;
        }
        self.heap.mem.set_u32(ch + CLOSED, 1);
        self.wake_all(ch, SENDERS);
        self.wake_all(ch, RECEIVERS);
    }

    /// The end of `channel_with`'s extent: the items dropped, the channel closed (`ch` borrowed).
    /// A topic subscription (`topic_subscribe`) leaves its topic here.
    pub fn channel_end(&mut self, ch: u32) {
        if self.sched.topics.len > 0 {
            self.topic_unregister(ch);
        }
        let mut items = Stack::load(&self.heap.mem, ch + ITEMS);
        let head = self.heap.mem.u32(ch + HEAD);
        let mut dead = 0;
        for i in head..items.len {
            let w = self.stack_at(&items, i);
            self.dec(w, &mut dead);
        }
        items.len = 0;
        items.store(&mut self.heap.mem, ch + ITEMS);
        self.heap.mem.set_u32(ch + HEAD, 0);
        self.release(dead);
        self.close_channel(ch);
    }

    /// Register the subscription channel `ch` under topic `name` (a `String`, consumed; ABI
    /// 1.15.0, `std/worker`). The registry (`Sched::topics`, `name, ch` pairs in subscription
    /// order) counts the name but not the channel, since `topic_subscribe`'s finalizer
    /// (`channel_end`) takes it out before the channel can die. The host is asked to deliver
    /// `name`'s messages from other instances (`topic_listen`).
    pub fn topic_register(&mut self, name: u32, ch: u32) -> Res<()> {
        let mut t = self.sched.topics;
        let mut r = self.push(&mut t, name);
        if r.is_ok() {
            r = self.push(&mut t, ch);
            if r.is_err() {
                t.len -= 1;
            }
        }
        self.sched.topics = t;
        if r.is_err() {
            self.drop(name);
            return r;
        }
        let (len, at) = self.data(name);
        self.host_topic_listen(at, len);
        Ok(())
    }

    /// Take subscription `ch` out of the registry (`channel_end`), its name released.
    fn topic_unregister(&mut self, ch: u32) {
        let mut t = self.sched.topics;
        let Some(i) = (0..t.len)
            .step_by(2)
            .find(|&i| self.stack_at(&t, i + 1) == ch)
        else {
            return;
        };
        let name = self.stack_at(&t, i);
        for j in i..t.len - 2 {
            let w = self.stack_at(&t, j + 2);
            self.stack_set(&t, j, w);
        }
        t.len -= 2;
        self.sched.topics = t;
        self.drop(name);
    }

    /// Whether registered name `name` holds the `len` bytes at `at`.
    fn topic_is(&self, name: u32, at: u32, len: u32) -> bool {
        let (nl, na) = self.data(name);
        nl == len && self.heap.mem.bytes(na, nl) == self.heap.mem.bytes(at, len)
    }

    /// Queue the string `text` (borrowed) on every subscription of the topic named by the `len`
    /// bytes at `at`, in the order they subscribed: 1 when one was full (it misses `text`),
    /// else 0. Queueing wakes receivers and runs no code, so the registry cannot change here.
    fn topic_fanout(&mut self, at: u32, len: u32, text: u32) -> Res<i64> {
        let mut full = 0;
        let mut i = 0;
        while i < self.sched.topics.len {
            let t = self.sched.topics;
            let (name, ch) = (self.stack_at(&t, i), self.stack_at(&t, i + 1));
            i += 2;
            if !self.topic_is(name, at, len) {
                continue;
            }
            self.dup(ch)?;
            self.dup(text).inspect_err(|_| self.drop(ch))?;
            if self.channel_try_send(ch, text)? == FULL {
                full = 1;
            }
        }
        Ok(full)
    }

    /// `topic_publish(name, text)` (both consumed): `topic_fanout` here, and the message to the
    /// host for the subscriptions of other instances (`topic_post`), which are not counted.
    pub fn topic_publish(&mut self, name: u32, text: u32) -> Res<i64> {
        let (nl, na) = self.data(name);
        let r = self.topic_fanout(na, nl, text);
        if r.is_ok() {
            let (tl, ta) = self.data(text);
            self.host_topic_post(na, nl, ta, tl);
        }
        self.drop(name);
        self.drop(text);
        r
    }

    /// A message the host received for a topic from another instance (`topic_deliver`): the
    /// name and the UTF-8 text are `(len, at)` slices, as [`Rt::data`] gives them; queued as
    /// `topic_publish` queues one published here.
    pub fn topic_deliver(&mut self, name: (u32, u32), text: (u32, u32)) -> Res<()> {
        let ((nl, na), (tl, ta)) = (name, text);
        let t = self.sched.topics;
        let any = (0..t.len)
            .step_by(2)
            .any(|i| self.topic_is(self.stack_at(&t, i), na, nl));
        if !any {
            return Ok(());
        }
        let s = self.new_object(
            tag::STRING,
            tl.checked_add(16).ok_or(Defect::HEAP_EXHAUSTED)?,
        )?;
        self.heap.mem.set_u32(s + 8, tl);
        self.heap.mem.copy(s + 16, ta, tl);
        let r = self.topic_fanout(na, nl, s);
        self.drop(s);
        r.map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    fn host_topic_listen(&mut self, at: u32, len: u32) {
        // SAFETY: a plain import with a slice inside linear memory, copied in at once.
        unsafe { crate::host::topic_listen(at, len) }
    }

    #[cfg(target_arch = "wasm32")]
    fn host_topic_post(&mut self, na: u32, nl: u32, ta: u32, tl: u32) {
        // SAFETY: a plain import with two slices inside linear memory, copied in at once.
        unsafe { crate::host::topic_post(na, nl, ta, tl) }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn host_topic_listen(&mut self, at: u32, len: u32) {
        let name = self.heap.mem.bytes(at, len).to_vec();
        self.topic_log.push(crate::TopicCall::Listen(name));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn host_topic_post(&mut self, na: u32, nl: u32, ta: u32, tl: u32) {
        let name = self.heap.mem.bytes(na, nl).to_vec();
        let text = self.heap.mem.bytes(ta, tl).to_vec();
        self.topic_log.push(crate::TopicCall::Post(name, text));
    }

    /// Release a dead channel's items and lists (`object.rs`, the destruction worklist).
    pub(crate) fn channel_release(&mut self, ch: u32, head: &mut u32) {
        let items = Stack::load(&self.heap.mem, ch + ITEMS);
        for i in self.heap.mem.u32(ch + HEAD)..items.len {
            let w = self.stack_at(&items, i);
            self.dec(w, head);
        }
        for off in [ITEMS, SENDERS, RECEIVERS] {
            let s = Stack::load(&self.heap.mem, ch + off);
            self.stack_free(s);
        }
    }
}
