use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll, RawWaker, RawWakerVTable, Waker};

use boa_engine::job::{GenericJob, IntervalJob, Job, JobExecutor, NativeAsyncJob, PromiseJob, TimeoutJob};
use boa_engine::{Context, JsError, JsResult, JsValue};

fn dummy_waker() -> Waker {
    fn noop(_: *const ()) {}
    fn clone(p: *const ()) -> RawWaker {
        RawWaker::new(p, &VTABLE)
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
}

use std::sync::atomic::{AtomicPtr, Ordering};

static DUMMY_PTR: AtomicPtr<Context> = AtomicPtr::new(std::ptr::null_mut());

/// A lazily-initialized static default `Context` used as a safe, non-dangling placeholder
/// in `HostJobExecutor::context_ref` whenever `run_jobs` is not actively executing.
fn dummy_context() -> &'static mut Context {
    let mut ptr = DUMMY_PTR.load(Ordering::Acquire);
    if ptr.is_null() {
        let new_ptr = Box::into_raw(Box::new(Context::default()));
        match DUMMY_PTR.compare_exchange(
            std::ptr::null_mut(),
            new_ptr,
            Ordering::Release,
            Ordering::Acquire,
        ) {
            Ok(_) => ptr = new_ptr,
            Err(actual) => {
                // Lost race, reclaim newly allocated box
                unsafe { drop(Box::from_raw(new_ptr)); }
                ptr = actual;
            }
        }
    }
    unsafe { &mut *ptr }
}

/// RAII guard that resets the executor's `context_ref` slot back to `dummy_context()`
/// upon exit from `run_jobs`, ensuring caller stack references never outlive the call.
struct ContextResetGuard<'a>(&'a RefCell<&'static mut Context>);

impl<'a> Drop for ContextResetGuard<'a> {
    fn drop(&mut self) {
        *self.0.borrow_mut() = dummy_context();
    }
}

pub(crate) struct HostJobExecutor {
    context_ref: &'static RefCell<&'static mut Context>,
    promise_jobs: RefCell<VecDeque<PromiseJob>>,
    generic_jobs: RefCell<VecDeque<GenericJob>>,
    async_jobs: RefCell<VecDeque<NativeAsyncJob>>,
    timeout_jobs: RefCell<Vec<TimeoutJob>>,
    interval_jobs: RefCell<Vec<IntervalJob>>,
    running_futures: RefCell<Vec<Pin<Box<dyn Future<Output = JsResult<JsValue>>>>>>,
}

impl Drop for HostJobExecutor {
    fn drop(&mut self) {
        // 1. Drop any running futures before reclaiming the context cell
        self.running_futures.borrow_mut().clear();
        // 2. Reclaim the heap-allocated cell so it is freed with the executor (zero per-runtime leak)
        unsafe {
            drop(Box::from_raw(self.context_ref as *const _ as *mut RefCell<&'static mut Context>));
        }
    }
}

impl Default for HostJobExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl HostJobExecutor {
    pub fn new() -> Self {
        let cell = Box::leak(Box::new(RefCell::new(dummy_context())));
        Self {
            context_ref: cell,
            promise_jobs: RefCell::default(),
            generic_jobs: RefCell::default(),
            async_jobs: RefCell::default(),
            timeout_jobs: RefCell::default(),
            interval_jobs: RefCell::default(),
            running_futures: RefCell::default(),
        }
    }
}

impl JobExecutor for HostJobExecutor {
    fn enqueue_job(self: Rc<Self>, job: Job, _context: &mut Context) {
        match job {
            Job::PromiseJob(p) => self.promise_jobs.borrow_mut().push_back(p),
            Job::AsyncJob(a) => self.async_jobs.borrow_mut().push_back(a),
            Job::GenericJob(g) => self.generic_jobs.borrow_mut().push_back(g),
            Job::TimeoutJob(t) => self.timeout_jobs.borrow_mut().push(t),
            Job::IntervalJob(i) => self.interval_jobs.borrow_mut().push(i),
            Job::FinalizationRegistryCleanupJob(fr) => self.async_jobs.borrow_mut().push_back(fr),
            other => {
                // In RustKit, timer scheduling (setInterval / setTimeout) is driven
                // by the host DomBindings event loop (run_timers). Any other unrecognized
                // job variant is logged and ignored.
                let _ = other;
            }
        }
    }

    fn run_jobs(self: Rc<Self>, context: &mut Context) -> JsResult<()> {
        // SAFETY: We temporarily stash a mutable reference to `context` into `context_ref`
        // so that async module-load jobs can access it via Boa's job callback API.
        // Aliasing is prevented because ALL accesses to Context in run_jobs (including
        // draining promise jobs, generic jobs, and timeouts) are routed exclusively through
        // `self.context_ref.borrow_mut()`, ensuring at most one active `&mut Context` at a time.
        // The `ContextResetGuard` guarantees that `context_ref` is reset back to `dummy_context()`
        // when `run_jobs` returns (whether normally or via error), ensuring the reference to
        // `context` never outlives this stack frame.
        *self.context_ref.borrow_mut() = unsafe { std::mem::transmute(&mut *context) };
        let _guard = ContextResetGuard(self.context_ref);

        let waker = dummy_waker();
        let mut cx = TaskContext::from_waker(&waker);
        let mut first_error: Option<JsError> = None;

        loop {
            let mut progress = false;

            // 1. Take any newly queued async jobs and start them
            let new_async = std::mem::take(&mut *self.async_jobs.borrow_mut());
            for job in new_async {
                let fut = job.call(self.context_ref);
                self.running_futures.borrow_mut().push(Box::pin(fut));
                progress = true;
            }

            // 2. Poll running futures
            {
                let mut futures = self.running_futures.borrow_mut();
                let mut i = 0;
                while i < futures.len() {
                    match futures[i].as_mut().poll(&mut cx) {
                        Poll::Ready(Ok(_)) => {
                            drop(futures.swap_remove(i));
                            progress = true;
                        }
                        Poll::Ready(Err(e)) => {
                            drop(futures.swap_remove(i));
                            if first_error.is_none() {
                                first_error = Some(e);
                            }
                            progress = true;
                        }
                        Poll::Pending => {
                            i += 1;
                        }
                    }
                }
            }

            // 3. Drain promise jobs through the context slot to avoid aliasing
            let promise_jobs = std::mem::take(&mut *self.promise_jobs.borrow_mut());
            for job in promise_jobs {
                if let Err(e) = job.call(&mut *self.context_ref.borrow_mut()) {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
                progress = true;
            }

            // 4. Drain generic jobs through the context slot
            let generic_jobs = std::mem::take(&mut *self.generic_jobs.borrow_mut());
            for job in generic_jobs {
                if let Err(e) = job.call(&mut *self.context_ref.borrow_mut()) {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
                progress = true;
            }

            // 5. Drain timeouts through the context slot
            let timeouts = std::mem::take(&mut *self.timeout_jobs.borrow_mut());
            for job in timeouts {
                if !job.cancelled() {
                    if let Err(e) = job.call(&mut *self.context_ref.borrow_mut()) {
                        if first_error.is_none() {
                            first_error = Some(e);
                        }
                    }
                    progress = true;
                }
            }

            if !progress {
                break;
            }
        }

        self.context_ref.borrow_mut().clear_kept_objects();
        if let Some(err) = first_error {
            Err(err)
        } else {
            Ok(())
        }
    }
}
