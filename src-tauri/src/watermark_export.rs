use crate::watermark_model::{
    PhotoPlacementOverride, WatermarkOutputSettings, WatermarkRenderRequest,
    WatermarkSourceSnapshot, WatermarkTemplate,
};
use crate::watermark_output::{
    PlannedWatermarkOutput, WatermarkOutputResult, WatermarkOutputStatus,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub(crate) struct WatermarkExportItem {
    pub(crate) plan: PlannedWatermarkOutput,
    pub(crate) request: WatermarkRenderRequest,
}

pub(crate) trait OutputExecutor: Send + Sync + 'static {
    fn execute(&self, item: &WatermarkExportItem) -> WatermarkOutputResult;
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WatermarkExportRequest {
    pub(crate) snapshot: WatermarkSourceSnapshot,
    pub(crate) settings: WatermarkOutputSettings,
    pub(crate) template: WatermarkTemplate,
    #[serde(default)]
    pub(crate) photo_overrides: BTreeMap<String, PhotoPlacementOverride>,
}

pub(crate) struct RealOutputExecutor {
    resource_dir: PathBuf,
}

impl RealOutputExecutor {
    pub(crate) fn new(resource_dir: PathBuf) -> Self {
        Self { resource_dir }
    }
}

impl OutputExecutor for RealOutputExecutor {
    fn execute(&self, item: &WatermarkExportItem) -> WatermarkOutputResult {
        crate::watermark_output::write_output(&item.plan, &item.request, &self.resource_dir)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatermarkExportSummary {
    pub(crate) total: usize,
    pub(crate) succeeded: usize,
    pub(crate) skipped: usize,
    pub(crate) failed: usize,
    pub(crate) cancelled: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum WatermarkExportEvent {
    Started {
        task_id: String,
        total: usize,
    },
    ItemStarted {
        task_id: String,
        photo_id: String,
        index: usize,
    },
    ItemFinished {
        task_id: String,
        result: WatermarkOutputResult,
    },
    Finished {
        task_id: String,
        summary: WatermarkExportSummary,
    },
}

#[derive(Default)]
pub(crate) struct ExportTaskControl {
    cancelled: AtomicBool,
    next: AtomicUsize,
}

impl ExportTaskControl {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn claim(&self, total: usize) -> Option<usize> {
        if self.cancelled.load(Ordering::SeqCst) {
            return None;
        }
        let position = self.next.fetch_add(1, Ordering::SeqCst);
        if position >= total || self.cancelled.load(Ordering::SeqCst) {
            None
        } else {
            Some(position)
        }
    }
}

pub(crate) struct WatermarkExportTask {
    task_id: String,
    items: Vec<WatermarkExportItem>,
    output_directory: PathBuf,
    results: Mutex<Vec<Option<WatermarkOutputResult>>>,
    control: Mutex<Arc<ExportTaskControl>>,
    running: AtomicBool,
    completed: AtomicBool,
}

impl WatermarkExportTask {
    fn new(task_id: &str, items: Vec<WatermarkExportItem>) -> Result<Self, String> {
        if task_id.trim().is_empty() || task_id.len() > 128 {
            return Err("水印导出任务 ID 无效".into());
        }
        let output_directory = items
            .first()
            .map(|item| item.plan.output_directory().to_path_buf())
            .ok_or_else(|| "水印导出任务没有可执行照片".to_string())?;
        if items
            .iter()
            .any(|item| item.plan.output_directory() != output_directory)
        {
            return Err("同一水印导出任务必须使用统一输出目录".into());
        }
        let result_count = items.len();
        Ok(Self {
            task_id: task_id.to_string(),
            items,
            output_directory,
            results: Mutex::new(vec![None; result_count]),
            control: Mutex::new(Arc::new(ExportTaskControl::default())),
            running: AtomicBool::new(false),
            completed: AtomicBool::new(false),
        })
    }

    pub(crate) fn cancel(&self) {
        if let Ok(control) = self.control.lock() {
            control.cancel();
        }
    }

    #[allow(dead_code)]
    pub(crate) fn results(&self) -> Vec<WatermarkOutputResult> {
        self.results
            .lock()
            .map(|results| results.iter().filter_map(Clone::clone).collect())
            .unwrap_or_default()
    }

    pub(crate) fn failed_indices(&self) -> Vec<usize> {
        self.results
            .lock()
            .map(|results| {
                results
                    .iter()
                    .enumerate()
                    .filter_map(|(index, result)| {
                        result
                            .as_ref()
                            .is_some_and(|item| item.status == WatermarkOutputStatus::Failed)
                            .then_some(index)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn is_completed(&self) -> bool {
        self.completed.load(Ordering::SeqCst)
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub(crate) fn output_directory(&self) -> &Path {
        &self.output_directory
    }

    pub(crate) fn item_count(&self) -> usize {
        self.items.len()
    }
}

#[derive(Default)]
pub(crate) struct WatermarkExportStore {
    tasks: Mutex<HashMap<String, Arc<WatermarkExportTask>>>,
}

impl WatermarkExportStore {
    pub(crate) fn task(
        task_id: &str,
        items: Vec<WatermarkExportItem>,
    ) -> Result<Arc<WatermarkExportTask>, String> {
        Ok(Arc::new(WatermarkExportTask::new(task_id, items)?))
    }

    pub(crate) fn insert(
        &self,
        task_id: &str,
        items: Vec<WatermarkExportItem>,
    ) -> Result<Arc<WatermarkExportTask>, String> {
        let task = Self::task(task_id, items)?;
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| "水印导出任务状态异常".to_string())?;
        if tasks.contains_key(task_id) {
            return Err("同名水印导出任务已经存在".into());
        }
        tasks.insert(task_id.to_string(), task.clone());
        Ok(task)
    }

    pub(crate) fn get(&self, task_id: &str) -> Result<Arc<WatermarkExportTask>, String> {
        self.tasks
            .lock()
            .map_err(|_| "水印导出任务状态异常".to_string())?
            .get(task_id)
            .cloned()
            .ok_or_else(|| "水印导出任务不存在或已清除".to_string())
    }

    pub(crate) fn acknowledge(&self, task_id: &str) -> Result<(), String> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| "水印导出任务状态异常".to_string())?;
        let task = tasks
            .get(task_id)
            .ok_or_else(|| "水印导出任务不存在或已清除".to_string())?;
        if task.is_running() || !task.is_completed() {
            return Err("水印导出任务尚未结束".into());
        }
        tasks.remove(task_id);
        Ok(())
    }

    pub(crate) fn completed_output_directory(&self, task_id: &str) -> Result<PathBuf, String> {
        let task = self.get(task_id)?;
        if task.is_running() || !task.is_completed() {
            return Err("水印导出任务尚未结束".into());
        }
        Ok(task.output_directory().to_path_buf())
    }
}

pub(crate) fn default_export_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(|value| (value.get() / 2).clamp(1, 4))
        .unwrap_or(1)
}

pub(crate) fn validate_disk_space(required: u64, available: u64) -> Result<(), String> {
    if available < required {
        return Err(format!(
            "输出目录空间不足：预计最多需要 {required} 字节，可用 {available} 字节"
        ));
    }
    Ok(())
}

pub(crate) fn build_export_items(
    request: &WatermarkExportRequest,
) -> Result<Vec<WatermarkExportItem>, String> {
    crate::watermark_model::validate_template(&request.template)?;
    let photo_ids = request
        .snapshot
        .photos
        .iter()
        .map(|photo| photo.id.as_str())
        .collect::<HashSet<_>>();
    if request
        .photo_overrides
        .keys()
        .any(|photo_id| !photo_ids.contains(photo_id.as_str()))
    {
        return Err("单张照片调整包含当前任务之外的照片".into());
    }
    crate::watermark_output::plan_outputs(&request.snapshot, &request.settings)?
        .into_iter()
        .map(|plan| {
            let render_request = WatermarkRenderRequest {
                schema_version: crate::watermark_model::WATERMARK_SCHEMA_VERSION,
                source: plan.photo.clone(),
                template: request.template.clone(),
                photo_override: request.photo_overrides.get(&plan.photo.id).cloned(),
                color_space: request.settings.color_space,
                transparent_background: request.settings.transparent_background,
                jpeg_flatten_color: request.settings.jpeg_flatten_color.clone(),
            };
            Ok(WatermarkExportItem {
                plan,
                request: render_request,
            })
        })
        .collect()
}

pub(crate) fn preflight_disk_space(items: &[WatermarkExportItem]) -> Result<(), String> {
    let first = items
        .first()
        .ok_or_else(|| "没有可导出的水印照片".to_string())?;
    let output_directory = first.plan.output_directory();
    let probe_path = if output_directory.exists() {
        output_directory
    } else {
        output_directory
            .parent()
            .ok_or_else(|| "无法确定输出目录上级路径".to_string())?
    };
    let required = items.iter().try_fold(0u64, |total, item| {
        total
            .checked_add(item.plan.estimated_max_bytes())
            .ok_or_else(|| "输出空间估算溢出".to_string())
    })?;
    let available = fs2::available_space(probe_path)
        .map_err(|error| format!("无法读取输出目录可用空间：{error}"))?;
    validate_disk_space(required, available)
}

fn panic_result(item: &WatermarkExportItem) -> WatermarkOutputResult {
    WatermarkOutputResult {
        photo_id: item.plan.photo.id.clone(),
        target_path: item.plan.target_path.to_string_lossy().into_owned(),
        status: WatermarkOutputStatus::Failed,
        message: "水印导出任务异常结束".into(),
        size_bytes: None,
    }
}

fn summary(task: &WatermarkExportTask) -> WatermarkExportSummary {
    let results = task.results.lock().ok();
    let mut summary = WatermarkExportSummary {
        total: task.items.len(),
        succeeded: 0,
        skipped: 0,
        failed: 0,
        cancelled: 0,
    };
    if let Some(results) = results.as_deref() {
        for result in results {
            match result.as_ref().map(|item| item.status) {
                Some(WatermarkOutputStatus::Succeeded) => summary.succeeded += 1,
                Some(WatermarkOutputStatus::Skipped) => summary.skipped += 1,
                Some(WatermarkOutputStatus::Failed) => summary.failed += 1,
                None => summary.cancelled += 1,
            }
        }
    }
    summary
}

pub(crate) fn run_export_task(
    task: Arc<WatermarkExportTask>,
    indices: Vec<usize>,
    executor: Arc<dyn OutputExecutor>,
    on_event: Arc<dyn Fn(WatermarkExportEvent) + Send + Sync>,
    concurrency: usize,
) -> Result<(), String> {
    if indices.is_empty() {
        return Err("没有可执行或可重试的水印照片".into());
    }
    if indices.iter().any(|index| *index >= task.items.len()) {
        return Err("水印导出任务索引无效".into());
    }
    task.running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .map_err(|_| "水印导出任务正在运行".to_string())?;
    let control = if task.completed.swap(false, Ordering::SeqCst) {
        let next = Arc::new(ExportTaskControl::default());
        *task
            .control
            .lock()
            .map_err(|_| "水印导出取消状态异常".to_string())? = next.clone();
        next
    } else {
        task.control
            .lock()
            .map_err(|_| "水印导出取消状态异常".to_string())?
            .clone()
    };
    on_event(WatermarkExportEvent::Started {
        task_id: task.task_id.clone(),
        total: indices.len(),
    });
    let worker_count = concurrency.clamp(1, 4).min(indices.len());
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let task = task.clone();
            let control = control.clone();
            let executor = executor.clone();
            let on_event = on_event.clone();
            let indices = &indices;
            scope.spawn(move || {
                while let Some(position) = control.claim(indices.len()) {
                    let index = indices[position];
                    let item = &task.items[index];
                    on_event(WatermarkExportEvent::ItemStarted {
                        task_id: task.task_id.clone(),
                        photo_id: item.plan.photo.id.clone(),
                        index,
                    });
                    let result = catch_unwind(AssertUnwindSafe(|| executor.execute(item)))
                        .unwrap_or_else(|_| panic_result(item));
                    if let Ok(mut results) = task.results.lock() {
                        results[index] = Some(result.clone());
                    }
                    on_event(WatermarkExportEvent::ItemFinished {
                        task_id: task.task_id.clone(),
                        result,
                    });
                }
            });
        }
    });
    task.running.store(false, Ordering::SeqCst);
    task.completed.store(true, Ordering::SeqCst);
    if let Ok(mut current) = task.control.lock() {
        *current = control;
    }
    on_event(WatermarkExportEvent::Finished {
        task_id: task.task_id.clone(),
        summary: summary(&task),
    });
    Ok(())
}
