//! Tauri 托管的进程内状态。
//!
//! 这些 Store 都是「一次性计划」模型的载体：生成计划时写入，执行时取走。
//! 任何新增的危险操作都应当遵循同样的模式，而不是让前端直接传路径。

use crate::fs_util::now_ms;
use crate::pair_scan::ScanMode;
use crate::rating_sync;
use crate::safety::CleanupPlan;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(crate) struct ScanPlanStore {
    pub(crate) current: Mutex<Option<CurrentPlan>>,
}

#[derive(Default)]
pub(crate) struct RatingStore {
    pub(crate) access: Arc<Mutex<()>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PhotoRatingUpdate {
    pub(crate) asset_id: String,
    pub(crate) rating: u8,
    pub(crate) auto_sync: rating_sync::AutoSyncOutcome,
}

pub(crate) struct CurrentPlan {
    pub(crate) cleanup: CleanupPlan,
    pub(crate) mode: ScanMode,
    pub(crate) audit_paths: Vec<String>,
}

pub(crate) fn next_plan_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{sequence}", now_ms(), std::process::id())
}
