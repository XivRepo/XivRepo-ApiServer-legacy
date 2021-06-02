use actix_rt::time;
use actix_rt::Arbiter;
use futures::StreamExt;

pub struct Scheduler {
    arbiter: Arbiter,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            arbiter: Arbiter::new(),
        }
    }

    pub fn run<F, R>(&mut self, interval: std::time::Duration, mut task: F)
    where
        F: FnMut() -> R + Send + 'static,
        R: std::future::Future<Output = ()> + Send + 'static,
    {
        let future = time::interval(interval).for_each_concurrent(2, move |_| task());
        self.arbiter.send(future);
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.arbiter.stop();
    }
}

use thiserror::Error;

#[derive(Error, Debug)]
pub enum VersionIndexingError {
    #[error("Network error while updating game versions list: {0}")]
    NetworkError(#[from] reqwest::Error),
    #[error("Database error while updating game versions list: {0}")]
    DatabaseError(#[from] crate::database::models::DatabaseError),
}
