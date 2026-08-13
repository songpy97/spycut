use std::sync::Arc;

use serde::Serialize;
use tokio::sync::{Mutex, watch};

#[derive(Clone, Default)]
pub struct ExportRuntime {
    active: Arc<Mutex<Option<ActiveExport>>>,
}

#[derive(Clone)]
struct ActiveExport {
    id: String,
    cancel: watch::Sender<bool>,
}

pub struct ExportReservation {
    pub id: String,
    pub cancel_rx: watch::Receiver<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveExportProjection {
    pub job_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportRuntimeError {
    #[error("另一个导出任务正在运行")]
    AlreadyRunning,
    #[error("没有正在运行的导出任务")]
    NotRunning,
    #[error("导出任务编号不匹配")]
    JobMismatch,
}

impl ExportRuntime {
    pub async fn is_active(&self) -> bool {
        self.active.lock().await.is_some()
    }

    pub async fn reserve(&self) -> Result<ExportReservation, ExportRuntimeError> {
        let mut active = self.active.lock().await;
        if active.is_some() {
            return Err(ExportRuntimeError::AlreadyRunning);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let (cancel, cancel_rx) = watch::channel(false);
        *active = Some(ActiveExport {
            id: id.clone(),
            cancel,
        });
        Ok(ExportReservation { id, cancel_rx })
    }

    pub async fn cancel(&self, job_id: &str) -> Result<(), ExportRuntimeError> {
        let active = self.active.lock().await;
        let active = active.as_ref().ok_or(ExportRuntimeError::NotRunning)?;
        if active.id != job_id {
            return Err(ExportRuntimeError::JobMismatch);
        }
        let _ = active.cancel.send(true);
        Ok(())
    }

    pub async fn current(&self) -> Option<ActiveExportProjection> {
        self.active
            .lock()
            .await
            .as_ref()
            .map(|active| ActiveExportProjection {
                job_id: active.id.clone(),
            })
    }

    pub async fn release(&self, job_id: &str) {
        let mut active = self.active.lock().await;
        if active.as_ref().is_some_and(|item| item.id == job_id) {
            *active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn only_one_export_can_be_active() {
        let runtime = ExportRuntime::default();
        assert!(!runtime.is_active().await);
        let reservation = runtime.reserve().await.unwrap();
        assert!(runtime.is_active().await);
        assert!(matches!(
            runtime.reserve().await,
            Err(ExportRuntimeError::AlreadyRunning)
        ));
        runtime.cancel(&reservation.id).await.unwrap();
        assert!(*reservation.cancel_rx.borrow());
        runtime.release(&reservation.id).await;
        assert!(!runtime.is_active().await);
        assert!(runtime.current().await.is_none());
    }
}
