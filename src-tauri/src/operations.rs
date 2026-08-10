use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock,
    },
};

use parking_lot::Mutex;

use crate::error::{Error, Result};

static OPERATIONS: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct OperationGuard {
    id: String,
    cancelled: Arc<AtomicBool>,
}

pub fn begin(id: &str) -> OperationGuard {
    let cancelled = Arc::new(AtomicBool::new(false));
    OPERATIONS.lock().insert(id.to_owned(), cancelled.clone());
    OperationGuard {
        id: id.to_owned(),
        cancelled,
    }
}

pub fn cancel(id: &str) -> bool {
    OPERATIONS.lock().get(id).is_some_and(|cancelled| {
        cancelled.store(true, Ordering::Release);
        true
    })
}

pub fn check(id: &str) -> Result<()> {
    if OPERATIONS
        .lock()
        .get(id)
        .is_some_and(|cancelled| cancelled.load(Ordering::Acquire))
    {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

pub async fn cancelled(id: &str) {
    loop {
        if check(id).is_err() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        let mut operations = OPERATIONS.lock();
        if operations
            .get(&self.id)
            .is_some_and(|current| Arc::ptr_eq(current, &self.cancelled))
        {
            operations.remove(&self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_lives_only_for_the_registered_operation() {
        let id = uuid::Uuid::new_v4().to_string();
        let guard = begin(&id);
        assert!(check(&id).is_ok());
        assert!(cancel(&id));
        assert!(matches!(check(&id), Err(Error::Cancelled)));
        drop(guard);
        assert!(!cancel(&id));
    }
}
