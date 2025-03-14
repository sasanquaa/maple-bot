use std::{fmt, time::Duration};

use tokio::{
    spawn,
    sync::oneshot::{self, Receiver, error::TryRecvError},
    task::spawn_blocking,
    time::{self},
};

use anyhow::Result;

#[derive(Debug)]
pub struct Task<T> {
    rx: Receiver<T>,
    completed: bool,
}

#[derive(Copy, Clone, Debug)]
enum TaskState<T> {
    Complete(T),
    AlreadyCompleted,
    Pending,
    Error,
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

    fn spawn_blocking<F>(f: F) -> Task<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        spawn_blocking(move || {
            let _ = tx.send(f());
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

    fn poll_inner(&mut self) -> TaskState<T> {
        if self.completed {
            return TaskState::AlreadyCompleted;
        }
        debug_assert!(!self.completed);
        let state = match self.rx.try_recv() {
            Ok(value) => TaskState::Complete(value),
            Err(TryRecvError::Empty) => TaskState::Pending,
            Err(TryRecvError::Closed) => TaskState::Error,
        };
        if matches!(state, TaskState::Complete(_)) {
            self.completed = true;
        }
        state
    }
}

#[derive(Debug)]
pub enum Update<T> {
    Complete(T),
    Pending,
}

#[inline]
pub fn update_task_repeatable<F, T>(
    repeat_millis: u64,
    task: &mut Option<Task<Result<T>>>,
    task_fn: F,
) -> Update<Result<T>>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + fmt::Debug + 'static,
{
    match task.as_mut().map(|task| task.poll_inner()) {
        Some(TaskState::Error) | Some(TaskState::AlreadyCompleted) => {
            *task = Some(Task::spawn(async move {
                time::sleep(Duration::from_millis(repeat_millis)).await;
                spawn_blocking(task_fn).await.unwrap()
            }));
            Update::Pending
        }
        Some(TaskState::Complete(value)) => Update::Complete(value),
        Some(TaskState::Pending) => Update::Pending,
        None => {
            *task = Some(Task::spawn_blocking(task_fn));
            Update::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::task::{Task, TaskState};

    #[tokio::test(start_paused = true)]
    async fn spawn() {
        let mut task = Task::spawn(async move { 0 });
        assert!(!task.completed());

        while !task.completed() {
            match task.poll_inner() {
                TaskState::Complete(value) => assert_eq!(value, 0),
                TaskState::Pending => continue,
                TaskState::Error => unreachable!(),
                TaskState::AlreadyCompleted => unreachable!(),
            };
        }
        assert!(matches!(task.poll_inner(), TaskState::AlreadyCompleted));
        assert!(task.completed());
    }

    #[tokio::test(start_paused = true)]
    async fn spawn_blocking() {
        let mut task = Task::spawn_blocking(|| 0);
        assert!(!task.completed());

        while !task.completed() {
            match task.poll_inner() {
                TaskState::Complete(value) => assert_eq!(value, 0),
                TaskState::Pending => continue,
                TaskState::Error => unreachable!(),
                TaskState::AlreadyCompleted => unreachable!(),
            };
        }
        assert!(matches!(task.poll_inner(), TaskState::AlreadyCompleted));
        assert!(task.completed());
    }
}
