//! Background work whose results come back to the UI thread.
//!
//! `spawn` runs `work` on a worker thread and later hands its result to
//! `then` together with the [`App`]. In tests (`set_inline(true)`), work runs
//! immediately and results are queued until [`drain`] delivers them, so tests
//! are deterministic and never touch a real event loop.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::app::App;

type Payload = Box<dyn Any + Send>;
type Continuation = Box<dyn FnOnce(&mut App, Payload)>;

static INLINE: AtomicBool = AtomicBool::new(false);

thread_local! {
    static APP: RefCell<Weak<RefCell<App>>> = RefCell::new(Weak::new());
    static PENDING: RefCell<HashMap<u64, Continuation>> = RefCell::new(HashMap::new());
    static READY: RefCell<VecDeque<(u64, Payload)>> = RefCell::new(VecDeque::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
}

/// Run work synchronously (tests) instead of on threads.
pub fn set_inline(inline: bool) {
    INLINE.store(inline, Ordering::SeqCst);
}

pub fn is_inline() -> bool {
    INLINE.load(Ordering::SeqCst)
}

/// Registers the app that continuations receive.
pub fn register(app: &Rc<RefCell<App>>) {
    APP.with(|a| *a.borrow_mut() = Rc::downgrade(app));
}

/// Registers `then` to receive one value posted to the returned id.
pub fn continuation<T: Send + 'static>(then: impl FnOnce(&mut App, T) + 'static) -> u64 {
    let id = NEXT_ID.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    PENDING.with(|p| {
        p.borrow_mut().insert(
            id,
            Box::new(move |app: &mut App, payload: Payload| {
                if let Ok(value) = payload.downcast::<T>() {
                    then(app, *value);
                }
            }),
        )
    });
    id
}

/// Runs `work` in the background, then `then(app, result)` on the UI thread.
pub fn spawn<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    then: impl FnOnce(&mut App, T) + 'static,
) {
    let id = continuation(then);
    if is_inline() {
        let result: Payload = Box::new(work());
        READY.with(|r| r.borrow_mut().push_back((id, result)));
    } else {
        std::thread::spawn(move || {
            let result: Payload = Box::new(work());
            post(id, result);
        });
    }
}

/// Sends a message from any thread to a continuation registered with
/// [`continuation`]; used for progress updates.
pub fn post(id: u64, payload: Payload) {
    if is_inline() {
        READY.with(|r| r.borrow_mut().push_back((id, payload)));
        return;
    }
    let _ = slint::invoke_from_event_loop(move || {
        READY.with(|r| r.borrow_mut().push_back((id, payload)));
        drain();
    });
}

/// Registers a reusable handler for messages posted to the returned id.
pub fn listener<T: Send + 'static>(mut handler: impl FnMut(&mut App, T) + 'static) -> u64 {
    let id = NEXT_ID.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    let handler = Rc::new(RefCell::new(move |app: &mut App, payload: Payload| {
        if let Ok(value) = payload.downcast::<T>() {
            handler(app, *value);
        }
    }));
    LISTENERS.with(|l| l.borrow_mut().insert(id, handler));
    id
}

/// Stops delivering to a listener.
pub fn forget(id: u64) {
    LISTENERS.with(|l| l.borrow_mut().remove(&id));
}

type Listener = Rc<RefCell<dyn FnMut(&mut App, Payload)>>;

thread_local! {
    static LISTENERS: RefCell<HashMap<u64, Listener>> = RefCell::new(HashMap::new());
}

/// Delivers queued results. Safe to call at any time; does nothing while the
/// app is busy (it is called again once the current handler returns).
pub fn drain() {
    let Some(app) = APP.with(|a| a.borrow().upgrade()) else { return };
    loop {
        let Ok(mut guard) = app.try_borrow_mut() else { return };
        let Some((id, payload)) = READY.with(|r| r.borrow_mut().pop_front()) else { return };
        if let Some(continuation) = PENDING.with(|p| p.borrow_mut().remove(&id)) {
            continuation(&mut guard, payload);
        } else if let Some(listener) = LISTENERS.with(|l| l.borrow().get(&id).cloned()) {
            (listener.borrow_mut())(&mut guard, payload);
        }
    }
}

/// Number of results waiting for delivery (tests).
pub fn queued() -> usize {
    READY.with(|r| r.borrow().len())
}
