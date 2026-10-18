use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{Duration, Instant};

use boa_engine::context::time::{JsDuration, JsInstant};
use boa_engine::job::{
    GenericJob, IntervalJob, Job, JobExecutor, NativeAsyncJob, PromiseJob, TimeoutJob,
};
use boa_engine::{Context, JsError, JsNativeError, JsResult, JsValue};
use tracing::warn;

use crate::job_census;

fn dummy_waker() -> Waker {
    fn noop(_: *const ()) {}
    fn clone(p: *const ()) -> RawWaker {
        RawWaker::new(p, &VTABLE)
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
}

/// RAII guard that resets the executor's `context_cell` slot back to its owned
/// placeholder upon exit from `run_jobs`, ensuring caller stack references never
/// outlive the call and that in-flight futures do not hold active borrows.
struct ContextResetGuard {
    cell: *mut RefCell<&'static mut Context>,
    placeholder: *mut Context,
}

impl Drop for ContextResetGuard {
    fn drop(&mut self) {
        if std::thread::panicking() {
            // Avoid double panic / process abort during stack unwinding
            if let Ok(mut slot) = unsafe { &*self.cell }.try_borrow_mut() {
                *slot = unsafe { &mut *self.placeholder };
            }
            return;
        }
        match unsafe { &*self.cell }.try_borrow_mut() {
            Ok(mut slot) => {
                *slot = unsafe { &mut *self.placeholder };
            }
            Err(_) => {
                panic!("ContextResetGuard invariant violated: a future or callback still holds an active borrow on the executor context cell");
            }
        }
    }
}

/// Default maximum number of loop iterations/pumps per `run_jobs` call.
pub const DEFAULT_MAX_JOB_ITERATIONS: u64 = 10_000;

pub(crate) struct HostJobExecutor {
    placeholder_context: *mut Context,
    context_cell: *mut RefCell<&'static mut Context>,
    promise_jobs: RefCell<VecDeque<PromiseJob>>,
    generic_jobs: RefCell<VecDeque<GenericJob>>,
    async_jobs: RefCell<VecDeque<NativeAsyncJob>>,
    /// `HostEnqueueTimeoutJob` / interval jobs. A timer task, not a
    /// microtask: `run_jobs` does not drain this map. The host runs what is
    /// due from the timer task source (`run_due_timer_jobs`).
    clock_jobs: RefCell<BTreeMap<JsInstant, Vec<TimerTask>>>,
    running_futures: RefCell<Vec<Pin<Box<dyn Future<Output = JsResult<JsValue>>>>>>,
    max_job_iterations: Cell<u64>,
    timeout: Cell<Option<Duration>>,
    /// True while the outermost `run_jobs` is draining. A nested call
    /// returns without running jobs: a microtask checkpoint must not nest.
    draining: Cell<bool>,
}

/// Clears `draining` when the outermost `run_jobs` leaves, including by
/// panic. A nested call returns before this exists, so it must not clear
/// the flag the outer call still owns.
struct DrainGuard<'a> {
    draining: &'a Cell<bool>,
}

impl Drop for DrainGuard<'_> {
    fn drop(&mut self) {
        self.draining.set(false);
    }
}

impl Drop for HostJobExecutor {
    fn drop(&mut self) {
        // 1. Clear queued jobs and drop running futures before reclaiming cells
        self.clear();
        // 2. Reclaim the heap-allocated cell and placeholder context (zero per-runtime leak)
        unsafe {
            drop(Box::from_raw(self.context_cell));
            drop(Box::from_raw(self.placeholder_context));
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
        Self::with_limits(DEFAULT_MAX_JOB_ITERATIONS, None)
    }

    pub fn with_limits(max_job_iterations: u64, timeout: Option<Duration>) -> Self {
        let placeholder = Box::into_raw(Box::new(Context::default()));
        let cell = Box::into_raw(Box::new(RefCell::new(unsafe { &mut *placeholder })));
        Self {
            placeholder_context: placeholder,
            context_cell: cell,
            promise_jobs: RefCell::default(),
            generic_jobs: RefCell::default(),
            async_jobs: RefCell::default(),
            clock_jobs: RefCell::default(),
            running_futures: RefCell::default(),
            max_job_iterations: Cell::new(max_job_iterations),
            timeout: Cell::new(timeout),
            draining: Cell::new(false),
        }
    }

    #[inline]
    fn context_ref(&self) -> &'static RefCell<&'static mut Context> {
        unsafe { &*self.context_cell }
    }

    pub fn set_max_job_iterations(&self, max: u64) {
        self.max_job_iterations.set(max);
    }

    #[allow(dead_code)]
    pub fn max_job_iterations(&self) -> u64 {
        self.max_job_iterations.get()
    }

    pub fn set_timeout(&self, timeout: Option<Duration>) {
        self.timeout.set(timeout);
    }

    #[allow(dead_code)]
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout.get()
    }

    /// Purge all queued jobs and running futures. On a runaway loop/timeout breach,
    /// dropping in-flight futures and queues contains the runaway and prevents
    /// further recursive job scheduling.
    pub fn clear(&self) {
        self.promise_jobs.borrow_mut().clear();
        self.generic_jobs.borrow_mut().clear();
        self.async_jobs.borrow_mut().clear();
        self.clock_jobs.borrow_mut().clear();
        self.running_futures.borrow_mut().clear();
    }
}

/// A Boa timer job waiting for its deadline. Not a microtask.
enum TimerTask {
    Timeout(TimeoutJob),
    Interval(IntervalJob),
}

impl TimerTask {
    fn cancelled(&self) -> bool {
        match self {
            TimerTask::Timeout(job) => job.cancelled(),
            TimerTask::Interval(job) => job.cancelled(),
        }
    }
}

/// Writes the census when the outermost `run_jobs` leaves, including by an
/// early return or a panic.
struct CensusClose<'a> {
    started: Instant,
    jobs: &'a Cell<u64>,
    promise: &'a Cell<u64>,
    async_continuations: &'a Cell<u64>,
    generic: &'a Cell<u64>,
}

impl Drop for CensusClose<'_> {
    fn drop(&mut self) {
        let census = job_census::finish(
            self.jobs.get(),
            self.promise.get(),
            self.async_continuations.get(),
            self.generic.get(),
            self.started.elapsed(),
        );
        // A short drain is the common case (one `.then` after a script).
        // A long one is the Mac gap: one checkpoint that did not return.
        if census.elapsed_ms >= 50 || census.jobs >= 200 {
            warn!(target: "hiwave::jobs", "{}", census.summary());
        }
    }
}

impl JobExecutor for HostJobExecutor {
    fn enqueue_job(self: Rc<Self>, job: Job, context: &mut Context) {
        match job {
            Job::PromiseJob(p) => self.promise_jobs.borrow_mut().push_back(p),
            Job::AsyncJob(a) => self.async_jobs.borrow_mut().push_back(a),
            Job::GenericJob(g) => self.generic_jobs.borrow_mut().push_back(g),
            // HTML timer task source (and Atomics.waitAsync's timeout), not
            // the microtask checkpoint. The delay is the job's own; running
            // it here would ignore that delay and would let a `setTimeout(0)`
            // scheduler spin inside one drain.
            Job::TimeoutJob(t) => self.schedule_timer(context, t.timeout(), TimerTask::Timeout(t)),
            Job::IntervalJob(i) => {
                self.schedule_timer(context, i.interval(), TimerTask::Interval(i))
            }
            Job::FinalizationRegistryCleanupJob(fr) => self.async_jobs.borrow_mut().push_back(fr),
            other => {
                // Page `setTimeout` / `setInterval` / `postMessage` are the
                // bindings' timer queue, not a Boa job. Anything else Boa
                // adds stays queued until a host turn knows its task source.
                let _ = other;
            }
        }
    }

    fn run_jobs(self: Rc<Self>, context: &mut Context) -> JsResult<()> {
        // HTML: while a checkpoint is already running, a nested one returns.
        // The outer loop runs the jobs it queued, after the current job
        // returns. Draining them here would run them on top of that job's
        // frames, and would borrow the context cell the outer job still
        // holds. `replace` leaves the flag set; only the outer guard
        // clears it.
        if self.draining.replace(true) {
            return Ok(());
        }
        let _drain = DrainGuard {
            draining: &self.draining,
        };

        let context_cell = self.context_cell;
        let placeholder = self.placeholder_context;

        // SAFETY: We temporarily stash a mutable reference to `context` into `context_cell`
        // so that async module-load jobs can access it via Boa's job callback API.
        // Aliasing and lifetime safety depend on the following invariants:
        // 1. ALL accesses to Context in run_jobs (including draining promise jobs, generic
        //    jobs, and timeouts) are routed exclusively through `self.context_ref().borrow_mut()`,
        //    ensuring at most one active `&mut Context` at a time.
        // 2. Each `HostJobExecutor` owns its own dedicated heap placeholder Context and cell,
        //    so multiple runtimes/executors never share placeholder or context state.
        // 3. Soundness across intermediate turns relies on `running_futures` being polled strictly
        //    inside `run_jobs` while the caller's stack context is installed in `context_cell`.
        //    Between `run_jobs` calls, futures remain dormant and do not poll or borrow.
        // 4. `ContextResetGuard` verifies via `try_borrow_mut()` that no future or callback holds
        //    an active borrow when `run_jobs` exits, and resets `context_cell` back to this
        //    executor's owned placeholder so the caller's stack borrow never outlives the call.
        *unsafe { &*context_cell }.borrow_mut() = unsafe { std::mem::transmute(&mut *context) };
        let _guard = ContextResetGuard {
            cell: context_cell,
            placeholder,
        };

        let waker = dummy_waker();
        let mut cx = TaskContext::from_waker(&waker);
        let mut first_error: Option<JsError> = None;
        let started = Instant::now();
        let mut iterations: u64 = 0;
        let total_jobs = Cell::new(0);
        let promise_jobs_run = Cell::new(0);
        let async_continuations = Cell::new(0);
        let generic_jobs_run = Cell::new(0);
        let max_jobs = self.max_job_iterations.get();
        job_census::begin();
        let _census = CensusClose {
            started,
            jobs: &total_jobs,
            promise: &promise_jobs_run,
            async_continuations: &async_continuations,
            generic: &generic_jobs_run,
        };

        loop {
            iterations += 1;
            if iterations > max_jobs {
                self.clear();
                self.context_ref().borrow_mut().clear_kept_objects();
                return Err(JsError::from(
                    JsNativeError::range().with_message(format!(
                        "Job queue iteration limit ({max_jobs}) exceeded (runaway promise/microtask recursion)"
                    )),
                ));
            }

            if let Some(timeout) = self.timeout.get() {
                if started.elapsed() >= timeout {
                    self.clear();
                    self.context_ref().borrow_mut().clear_kept_objects();
                    return Err(JsError::from(JsNativeError::range().with_message(format!(
                        "Job queue execution timeout ({timeout:?}) exceeded"
                    ))));
                }
            }

            let mut progress = false;

            // 1. Take any newly queued async jobs and start them.
            //    A module-load continuation is one of these. It is polled
            //    below; it is not a timer task.
            let new_async = std::mem::take(&mut *self.async_jobs.borrow_mut());
            for job in new_async {
                if !count_job(&total_jobs, max_jobs) {
                    self.clear();
                    self.context_ref().borrow_mut().clear_kept_objects();
                    return Err(job_limit(max_jobs));
                }
                let fut = job.call(self.context_ref());
                self.running_futures.borrow_mut().push(Box::pin(fut));
                progress = true;
            }

            // 2. Poll running futures. A ready one is the host continuation
            //    (module load, Atomics waiter) resuming inside this checkpoint.
            {
                let mut futures = self.running_futures.borrow_mut();
                let mut i = 0;
                while i < futures.len() {
                    let polled_at = Instant::now();
                    match futures[i].as_mut().poll(&mut cx) {
                        Poll::Ready(Ok(_)) => {
                            job_census::note_source("async continuation", polled_at.elapsed());
                            async_continuations.set(async_continuations.get().saturating_add(1));
                            drop(futures.swap_remove(i));
                            progress = true;
                        }
                        Poll::Ready(Err(e)) => {
                            job_census::note_source("async continuation", polled_at.elapsed());
                            async_continuations.set(async_continuations.get().saturating_add(1));
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

            // 3. Drain promise jobs through the context slot to avoid aliasing.
            let promise_jobs = std::mem::take(&mut *self.promise_jobs.borrow_mut());
            for job in promise_jobs {
                if !count_job(&total_jobs, max_jobs) {
                    self.clear();
                    self.context_ref().borrow_mut().clear_kept_objects();
                    return Err(job_limit(max_jobs));
                }
                promise_jobs_run.set(promise_jobs_run.get().saturating_add(1));
                let callbacks_before = job_census::callback_count();
                let job_started = Instant::now();
                let result = job.call(&mut *self.context_ref().borrow_mut());
                if job_census::callback_count() == callbacks_before {
                    job_census::note_source("promise reaction", job_started.elapsed());
                }
                if let Err(e) = result {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
                progress = true;
            }

            // 4. Drain generic jobs through the context slot. The host does
            //    not enqueue these as tasks; Boa uses them for promise-adjacent
            //    bookkeeping. A timer never arrives as a generic job.
            let generic_jobs = std::mem::take(&mut *self.generic_jobs.borrow_mut());
            for job in generic_jobs {
                if !count_job(&total_jobs, max_jobs) {
                    self.clear();
                    self.context_ref().borrow_mut().clear_kept_objects();
                    return Err(job_limit(max_jobs));
                }
                generic_jobs_run.set(generic_jobs_run.get().saturating_add(1));
                let callbacks_before = job_census::callback_count();
                let job_started = Instant::now();
                let result = job.call(&mut *self.context_ref().borrow_mut());
                if job_census::callback_count() == callbacks_before {
                    job_census::note_source("generic host job", job_started.elapsed());
                }
                if let Err(e) = result {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
                progress = true;
            }

            if !progress {
                break;
            }
        }

        self.context_ref().borrow_mut().clear_kept_objects();
        if let Some(err) = first_error {
            Err(err)
        } else {
            Ok(())
        }
    }
}

fn count_job(total: &Cell<u64>, max_jobs: u64) -> bool {
    let next = total.get().saturating_add(1);
    total.set(next);
    next <= max_jobs
}

fn job_limit(max_jobs: u64) -> JsError {
    JsError::from(
        JsNativeError::range()
            .with_message(format!("Job queue total job limit ({max_jobs}) exceeded")),
    )
}

impl HostJobExecutor {
    fn schedule_timer(&self, context: &mut Context, delay: JsDuration, task: TimerTask) {
        let when = context.clock().now() + delay;
        self.clock_jobs
            .borrow_mut()
            .entry(when)
            .or_default()
            .push(task);
    }

    /// Run timer tasks whose deadline has passed, then one microtask
    /// checkpoint for the reactions they queued. A delay of zero is due
    /// now, and still does not run inside a checkpoint that is already
    /// draining. Returns how many timer callbacks ran.
    pub(crate) fn run_due_timer_jobs(self: Rc<Self>, context: &mut Context) -> JsResult<u32> {
        if self.draining.get() {
            return Ok(0);
        }
        let now = context.clock().now();
        let due = {
            let mut waiting = self.clock_jobs.borrow_mut();
            let mut later = BTreeMap::new();
            let mut due = Vec::new();
            for (when, jobs) in std::mem::take(&mut *waiting) {
                if when <= now {
                    due.extend(jobs);
                } else {
                    later.insert(when, jobs);
                }
            }
            *waiting = later;
            due
        };
        let mut ran = 0u32;
        for task in due {
            if task.cancelled() {
                continue;
            }
            ran = ran.saturating_add(1);
            match task {
                TimerTask::Timeout(job) => {
                    if let Err(err) = job.call(context) {
                        return Err(err);
                    }
                }
                TimerTask::Interval(job) => {
                    if let Err(err) = job.call(context) {
                        return Err(err);
                    }
                    // Once per timer turn. A zero interval becomes 1 ms so
                    // this call cannot spin on a job that reschedules itself
                    // as already due.
                    if !job.cancelled() {
                        let delay_ms = job.interval().as_millis().max(1);
                        let next = context.clock().now() + JsDuration::from_millis(delay_ms);
                        self.clock_jobs
                            .borrow_mut()
                            .entry(next)
                            .or_default()
                            .push(TimerTask::Interval(job));
                    }
                }
            }
        }
        if ran > 0 {
            self.run_jobs(context)?;
        }
        Ok(ran)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use boa_engine::job::IntervalJob;
    use boa_engine::job::{Job, PromiseJob, TimeoutJob};
    use boa_engine::{Context, JsValue, Source};

    use super::*;

    fn context() -> (Rc<HostJobExecutor>, Context) {
        let executor = Rc::new(HostJobExecutor::new());
        let context = Context::builder()
            .job_executor(executor.clone())
            .build()
            .unwrap();
        (executor, context)
    }

    #[test]
    fn a_timeout_job_is_not_a_microtask_and_a_zero_delay_waits_for_the_timer_turn() {
        let (executor, mut context) = context();
        let ran = Rc::new(Cell::new(0u32));
        let flag = ran.clone();
        context.enqueue_job(
            TimeoutJob::from_duration(
                move |_| {
                    flag.set(flag.get() + 1);
                    Ok(JsValue::undefined())
                },
                Duration::ZERO,
            )
            .into(),
        );
        context
            .eval(Source::from_bytes(
                "Promise.resolve().then(function reaction() {});",
            ))
            .unwrap();
        context.run_jobs().unwrap();
        assert_eq!(
            ran.get(),
            0,
            "a due timeout does not run inside the checkpoint"
        );
        let census = crate::job_census::last_census();
        assert!(census.promise >= 1, "{census:?}");
        assert_eq!(
            executor.clone().run_due_timer_jobs(&mut context).unwrap(),
            1
        );
        assert_eq!(ran.get(), 1, "the timer turn runs it once");
        assert_eq!(
            executor.run_due_timer_jobs(&mut context).unwrap(),
            0,
            "a timeout does not repeat"
        );
    }

    #[test]
    fn a_zero_interval_runs_once_per_timer_turn() {
        let (executor, mut context) = context();
        let ran = Rc::new(Cell::new(0u32));
        let flag = ran.clone();
        context.enqueue_job(
            IntervalJob::from_duration(
                move |_| {
                    flag.set(flag.get() + 1);
                    Ok(JsValue::undefined())
                },
                Duration::ZERO,
            )
            .into(),
        );
        context.run_jobs().unwrap();
        assert_eq!(ran.get(), 0);
        assert_eq!(
            executor.clone().run_due_timer_jobs(&mut context).unwrap(),
            1
        );
        assert_eq!(ran.get(), 1);
        // Still inside the same millisecond: the reschedule is 1 ms out,
        // so this call cannot spin.
        assert_eq!(
            executor.clone().run_due_timer_jobs(&mut context).unwrap(),
            0
        );
        assert_eq!(ran.get(), 1);
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(executor.run_due_timer_jobs(&mut context).unwrap(), 1);
        assert_eq!(ran.get(), 2);
    }

    #[test]
    fn a_nested_checkpoint_does_not_run_jobs_the_current_job_just_queued() {
        let (executor, mut context) = context();
        let nested = Rc::new(Cell::new(0u32));
        let saw_during_nest = Rc::new(Cell::new(0u32));
        let nested_for_inner = nested.clone();
        let nested_after = nested.clone();
        let saw = saw_during_nest.clone();
        let exec = executor.clone();
        context.enqueue_job(Job::from(PromiseJob::new(move |context| {
            context.enqueue_job(Job::from(PromiseJob::new(move |_| {
                nested_for_inner.set(nested_for_inner.get() + 1);
                Ok(JsValue::undefined())
            })));
            // Re-entering the checkpoint from inside a job must return.
            // The outer drain runs the queued job after this one returns.
            let _ = exec.clone().run_jobs(context);
            saw.set(nested_after.get());
            Ok(JsValue::undefined())
        })));
        context.run_jobs().unwrap();
        assert_eq!(saw_during_nest.get(), 0, "the nested drain ran the new job");
        assert_eq!(nested.get(), 1, "the outer drain ran it afterwards");
    }
}
