use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll, RawWaker, RawWakerVTable, Waker};

use boa_engine::job::{GenericJob, IntervalJob, Job, JobExecutor, NativeAsyncJob, PromiseJob, TimeoutJob};
use boa_engine::{Context, JsResult, JsValue};

fn dummy_waker() -> Waker {
    fn noop(_: *const ()) {}
    fn clone(p: *const ()) -> RawWaker {
        RawWaker::new(p, &VTABLE)
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
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

impl Default for HostJobExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl HostJobExecutor {
    pub fn new() -> Self {
        let placeholder: &'static mut Context = unsafe { &mut *std::ptr::NonNull::<Context>::dangling().as_ptr() };
        let cell = Box::leak(Box::new(RefCell::new(placeholder)));
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
            _ => {}
        }
    }

    fn run_jobs(self: Rc<Self>, context: &mut Context) -> JsResult<()> {
        *self.context_ref.borrow_mut() = unsafe { std::mem::transmute(&mut *context) };
        let waker = dummy_waker();
        let mut cx = TaskContext::from_waker(&waker);

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
                            return Err(e);
                        }
                        Poll::Pending => {
                            i += 1;
                        }
                    }
                }
            }

            // 3. Drain promise jobs
            let promise_jobs = std::mem::take(&mut *self.promise_jobs.borrow_mut());
            for job in promise_jobs {
                job.call(context)?;
                progress = true;
            }

            // 4. Drain generic jobs
            let generic_jobs = std::mem::take(&mut *self.generic_jobs.borrow_mut());
            for job in generic_jobs {
                job.call(context)?;
                progress = true;
            }

            // 5. Drain timeouts
            let timeouts = std::mem::take(&mut *self.timeout_jobs.borrow_mut());
            for job in timeouts {
                if !job.cancelled() {
                    job.call(context)?;
                    progress = true;
                }
            }

            if !progress {
                break;
            }
        }

        context.clear_kept_objects();
        Ok(())
    }
}
