use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileSnapshot {
    size_bytes: u64,
    modified_ms: Option<u64>,
}

impl FileSnapshot {
    pub(crate) fn new(size_bytes: u64, modified_ms: Option<u64>) -> Self {
        Self {
            size_bytes,
            modified_ms,
        }
    }
}

#[derive(Debug)]
pub(crate) struct CleanupPlan {
    id: String,
    raw_root: PathBuf,
    candidates: HashMap<String, FileSnapshot>,
}

impl CleanupPlan {
    pub(crate) fn new(
        id: String,
        raw_root: PathBuf,
        candidates: impl IntoIterator<Item = (String, FileSnapshot)>,
    ) -> Self {
        Self {
            id,
            raw_root,
            candidates: candidates.into_iter().collect(),
        }
    }

    pub(crate) fn authorize(
        &self,
        plan_id: &str,
        raw_root: &Path,
        relative_path: &str,
        snapshot: &FileSnapshot,
    ) -> Result<(), String> {
        if plan_id != self.id {
            return Err("清理计划已失效，请重新扫描".to_string());
        }
        if raw_root != self.raw_root {
            return Err("RAW 源目录与当前扫描计划不一致".to_string());
        }
        let expected = self
            .candidates
            .get(relative_path)
            .ok_or_else(|| "文件不在当前扫描的清理计划中".to_string())?;
        if snapshot != expected {
            return Err("文件信息与当前扫描计划不一致，请重新扫描".to_string());
        }
        Ok(())
    }

    pub(crate) fn matches(&self, plan_id: &str, raw_root: &Path) -> bool {
        self.id == plan_id && self.raw_root == raw_root
    }
}

pub(crate) fn unique_keys(keys: impl IntoIterator<Item = String>) -> HashSet<String> {
    keys.into_iter().collect()
}
