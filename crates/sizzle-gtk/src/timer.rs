//! Cancelable one-shot main-loop timers.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;

/// A slot holding a single pending one-shot main-loop timer.
///
/// [`glib::SourceId::remove`] consumes the id, so a pending timer can't be
/// canceled by dropping a copy of it. This keeps the id in a shared cell that
/// can be cleared and re-armed, providing cancel/replace semantics on top of
/// [`glib::timeout_add_local_once`].
#[derive(Clone, Default)]
pub(crate) struct TimerSlot(Rc<Cell<Option<glib::SourceId>>>);

impl TimerSlot {
    /// Cancel the pending timer, if any.
    pub(crate) fn cancel(&self) {
        if let Some(id) = self.0.replace(None) {
            id.remove();
        }
    }

    /// Replace any pending timer with one that runs `f` once after `delay`.
    /// The slot is cleared before `f` runs.
    pub(crate) fn schedule<F>(&self, delay: Duration, f: F)
    where
        F: FnOnce() + 'static,
    {
        self.cancel();
        let slot = Rc::clone(&self.0);
        let id = glib::timeout_add_local_once(delay, move || {
            slot.set(None);
            f();
        });
        self.0.set(Some(id));
    }
}
