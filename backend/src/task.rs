use std::{fmt, time::Duration};

use anyhow::{Error, Result};
use tokio::{
    spawn,
    sync::oneshot::{self, Receiver},
    task::spawn_blocking,
    time::sleep,
};

use crate::{context::Context, detect::Detector};

/// An asynchronous task.
///
/// The task is a wrapper around `tokio::task::spawn` mainly for using
/// inside synchronous code to do blocking or expensive operation.
#[derive(Debug)]
pub struct Task<T> {
    rx: Receiver<T>,
    completed: bool,
}

impl<T: fmt::Debug> Task<T> {
    fn spawn<F>(f: F) -> Task<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        spawn(async move {
            let _ = tx.send(f.await);
        });
        Task {
            rx,
            completed: false,
        }
    }

    #[cfg(test)]
    pub fn completed(&self) -> bool {
        self.completed
    }

    fn poll_inner(&mut self) -> Option<T> {
        if self.completed {
            return None;
        }
        debug_assert!(!self.completed);
        let value = self.rx.try_recv().ok();
        self.completed = value.is_some();
        value
    }
}

#[derive(Debug)]
pub enum Update<T> {
    Ok(T),
    Err(Error),
    Pending,
}

#[inline]
pub fn update_task<F, T, A>(
    repeat_delay_millis: u64,
    task: &mut Option<Task<Result<T>>>,
    task_fn_args: impl FnOnce() -> A,
    task_fn: F,
) -> Update<T>
where
    F: FnOnce(A) -> Result<T> + Send + 'static,
    T: fmt::Debug + Send + 'static,
    A: Send + 'static,
{
    let update = match task.as_mut().and_then(|task| task.poll_inner()) {
        Some(Ok(value)) => Update::Ok(value),
        Some(Err(err)) => Update::Err(err),
        None => Update::Pending,
    };
    if matches!(update, Update::Pending) && task.as_ref().is_none_or(|task| task.completed) {
        let has_delay = task.as_ref().is_some_and(|task| task.completed);
        let args = task_fn_args();
        let spawned = Task::spawn(async move {
            if has_delay {
                sleep(Duration::from_millis(repeat_delay_millis)).await;
            }
            spawn_blocking(move || task_fn(args)).await.unwrap()
        });
        *task = Some(spawned);
    }
    update
}

#[inline]
pub fn update_detection_task<F, T>(
    context: &Context,
    repeat_delay_millis: u64,
    task: &mut Option<Task<Result<T>>>,
    task_fn: F,
) -> Update<T>
where
    F: FnOnce(Box<dyn Detector>) -> Result<T> + Send + 'static,
    T: fmt::Debug + Send + 'static,
{
    update_task(
        repeat_delay_millis,
        task,
        || context.detector_cloned_unwrap(),
        task_fn,
    )
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use anyhow::Result;
    use tokio::task::yield_now;

    use crate::task::{Task, Update, update_task};

    #[tokio::test(start_paused = true)]
    async fn spawn_state() {
        let mut task = Task::spawn(async move { 0 });
        assert!(!task.completed());

        while !task.completed() {
            match task.poll_inner() {
                Some(value) => assert_eq!(value, 0),
                None => yield_now().await,
            };
        }
        assert_matches!(task.poll_inner(), None);
        assert!(task.completed());
    }

    #[tokio::test(start_paused = true)]
    async fn update_task_repeatable_state() {
        let mut task = None::<Task<Result<u32>>>;
        assert!(task.is_none());

        assert_matches!(
            update_task(1000, &mut task, || (), |_| Ok(0)),
            Update::Pending
        );
        assert!(task.is_some());

        while !task.as_ref().unwrap().completed() {
            match update_task(1000, &mut task, || (), |_| Ok(0)) {
                Update::Ok(value) => assert!(value == 0),
                Update::Pending => yield_now().await,
                Update::Err(_) => unreachable!(),
            }
        }
        assert_matches!(task.as_mut().unwrap().poll_inner(), None);
        assert!(task.as_ref().unwrap().completed());

        assert_matches!(
            update_task(1000, &mut task, || (), |_| Ok(0)),
            Update::Pending
        );
        assert!(!task.as_ref().unwrap().completed());
    }
}
