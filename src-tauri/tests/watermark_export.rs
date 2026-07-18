#![allow(dead_code)]

#[path = "../src/formats.rs"]
mod formats;
#[path = "../src/watermark_color.rs"]
mod watermark_color;
#[path = "../src/watermark_export.rs"]
mod watermark_export;
#[path = "../src/watermark_geometry.rs"]
mod watermark_geometry;
#[path = "../src/watermark_metadata.rs"]
mod watermark_metadata;
#[path = "../src/watermark_model.rs"]
mod watermark_model;
#[path = "../src/watermark_output.rs"]
mod watermark_output;
#[path = "../src/watermark_render.rs"]
mod watermark_render;
#[path = "../src/watermark_source.rs"]
mod watermark_source;
#[path = "../src/watermark_text.rs"]
mod watermark_text;

use image::{Rgb, RgbImage};
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use watermark_export::{
    OutputExecutor, WatermarkExportEvent, WatermarkExportItem, WatermarkExportStore,
    default_export_concurrency, run_export_task, validate_disk_space,
};
use watermark_model::{
    CollisionPolicy, MetadataPolicy, OutputColorSpace, WATERMARK_SCHEMA_VERSION,
    WatermarkOutputFormat, WatermarkOutputSettings, WatermarkRenderRequest, WatermarkSizing,
    WatermarkSourceOrigin, default_template,
};
use watermark_output::{WatermarkOutputResult, WatermarkOutputStatus, plan_outputs};
use watermark_source::{SourceInput, WatermarkSourceRequest, prepare_source};

fn items(root: &Path, count: usize) -> Vec<WatermarkExportItem> {
    let source = root.join("source");
    let output = root.join("output");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&output).unwrap();
    for index in 0..count {
        RgbImage::from_pixel(32, 24, Rgb([index as u8, 80, 160]))
            .save(source.join(format!("photo-{index:02}.jpg")))
            .unwrap();
    }
    let snapshot = prepare_source(WatermarkSourceRequest {
        origin: WatermarkSourceOrigin::Directory,
        inputs: vec![SourceInput::Directory {
            path: source.to_string_lossy().into_owned(),
        }],
    })
    .unwrap();
    let settings = WatermarkOutputSettings {
        format: WatermarkOutputFormat::Jpeg,
        jpeg_quality: 90,
        sizing: WatermarkSizing::Original {
            allow_upscale: false,
        },
        color_space: OutputColorSpace::Srgb,
        transparent_background: false,
        jpeg_flatten_color: "#ffffff".into(),
        metadata_policy: MetadataPolicy::Remove,
        output_directory: Some(output.to_string_lossy().into_owned()),
        suffix: "_FramePair".into(),
        collision_policy: CollisionPolicy::Sequence,
    };
    plan_outputs(&snapshot, &settings)
        .unwrap()
        .into_iter()
        .map(|plan| {
            let request = WatermarkRenderRequest {
                schema_version: WATERMARK_SCHEMA_VERSION,
                source: plan.photo.clone(),
                template: default_template("test", "测试"),
                photo_override: None,
                color_space: settings.color_space,
                transparent_background: settings.transparent_background,
                jpeg_flatten_color: settings.jpeg_flatten_color.clone(),
            };
            WatermarkExportItem { plan, request }
        })
        .collect()
}

struct FakeExecutor {
    active: AtomicUsize,
    maximum_active: AtomicUsize,
    started: AtomicUsize,
    failures: Mutex<HashSet<String>>,
    delay: Duration,
}

impl FakeExecutor {
    fn new(failures: &[&str], delay: Duration) -> Self {
        Self {
            active: AtomicUsize::new(0),
            maximum_active: AtomicUsize::new(0),
            started: AtomicUsize::new(0),
            failures: Mutex::new(failures.iter().map(|value| value.to_string()).collect()),
            delay,
        }
    }
}

impl OutputExecutor for FakeExecutor {
    fn execute(&self, item: &WatermarkExportItem) -> WatermarkOutputResult {
        self.started.fetch_add(1, Ordering::SeqCst);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum_active.fetch_max(active, Ordering::SeqCst);
        thread::sleep(self.delay);
        self.active.fetch_sub(1, Ordering::SeqCst);
        let failed = self.failures.lock().unwrap().contains(&item.plan.photo.id);
        WatermarkOutputResult {
            photo_id: item.plan.photo.id.clone(),
            target_path: item.plan.target_path.to_string_lossy().into_owned(),
            status: if failed {
                WatermarkOutputStatus::Failed
            } else {
                WatermarkOutputStatus::Succeeded
            },
            message: if failed { "模拟失败" } else { "完成" }.into(),
            size_bytes: (!failed).then_some(128),
        }
    }
}

#[test]
fn bounded_queue_isolates_failures_and_keeps_stable_result_order() {
    let temp = tempfile::tempdir().unwrap();
    let work = items(temp.path(), 6);
    let failed_id = work[2].plan.photo.id.clone();
    let executor = Arc::new(FakeExecutor::new(&[&failed_id], Duration::from_millis(20)));
    let events = Arc::new(Mutex::new(Vec::new()));
    let task = WatermarkExportStore::task("task-1", work.clone()).unwrap();
    run_export_task(
        task.clone(),
        (0..work.len()).collect(),
        executor.clone(),
        {
            let events = events.clone();
            Arc::new(move |event| events.lock().unwrap().push(event))
        },
        2,
    )
    .unwrap();

    assert!(executor.maximum_active.load(Ordering::SeqCst) <= 2);
    let results = task.results();
    assert_eq!(results.len(), work.len());
    assert_eq!(
        results
            .iter()
            .map(|result| &result.photo_id)
            .collect::<Vec<_>>(),
        work.iter()
            .map(|item| &item.plan.photo.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| result.status == WatermarkOutputStatus::Failed)
            .count(),
        1
    );
    let recorded = events.lock().unwrap();
    assert!(matches!(
        recorded.first(),
        Some(WatermarkExportEvent::Started { total: 6, .. })
    ));
    assert!(matches!(
        recorded.last(),
        Some(WatermarkExportEvent::Finished { .. })
    ));
    assert_eq!(
        recorded
            .iter()
            .filter(|event| matches!(event, WatermarkExportEvent::ItemFinished { .. }))
            .count(),
        6
    );
}

#[test]
fn cancellation_stops_new_claims_but_keeps_active_results() {
    let temp = tempfile::tempdir().unwrap();
    let work = items(temp.path(), 10);
    let executor = Arc::new(FakeExecutor::new(&[], Duration::from_millis(80)));
    let task = WatermarkExportStore::task("task-cancel", work.clone()).unwrap();
    let running_task = task.clone();
    let running_executor = executor.clone();
    let handle = thread::spawn(move || {
        run_export_task(
            running_task,
            (0..work.len()).collect(),
            running_executor,
            Arc::new(|_| {}),
            2,
        )
        .unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while executor.started.load(Ordering::SeqCst) < 2 && Instant::now() < deadline {
        thread::yield_now();
    }
    task.cancel();
    handle.join().unwrap();
    assert_eq!(executor.started.load(Ordering::SeqCst), 2);
    assert_eq!(task.results().len(), 2);
    assert!(task.is_completed());

    let before = WatermarkExportStore::task("before", items(temp.path(), 2)).unwrap();
    before.cancel();
    let before_executor = Arc::new(FakeExecutor::new(&[], Duration::ZERO));
    run_export_task(
        before.clone(),
        vec![0, 1],
        before_executor.clone(),
        Arc::new(|_| {}),
        2,
    )
    .unwrap();
    assert_eq!(before_executor.started.load(Ordering::SeqCst), 0);
    assert!(before.results().is_empty());
}

#[test]
fn store_refuses_duplicates_retains_results_and_retries_only_failures() {
    let temp = tempfile::tempdir().unwrap();
    let work = items(temp.path(), 4);
    let failed_id = work[1].plan.photo.id.clone();
    let store = WatermarkExportStore::default();
    let task = store.insert("same", work.clone()).unwrap();
    assert!(store.insert("same", work).is_err());
    assert!(store.completed_output_directory("same").is_err());
    run_export_task(
        task.clone(),
        vec![0, 1, 2, 3],
        Arc::new(FakeExecutor::new(&[&failed_id], Duration::ZERO)),
        Arc::new(|_| {}),
        2,
    )
    .unwrap();
    assert_eq!(store.get("same").unwrap().results().len(), 4);
    let retry = task.failed_indices();
    assert_eq!(retry, vec![1]);
    run_export_task(
        task.clone(),
        retry,
        Arc::new(FakeExecutor::new(&[], Duration::ZERO)),
        Arc::new(|_| {}),
        1,
    )
    .unwrap();
    assert!(
        task.results()
            .iter()
            .all(|result| result.status == WatermarkOutputStatus::Succeeded)
    );
    assert_eq!(
        store.completed_output_directory("same").unwrap(),
        task.output_directory()
    );
    assert!(store.acknowledge("same").is_ok());
    assert!(store.get("same").is_err());
}

#[test]
fn disk_preflight_and_default_concurrency_are_bounded() {
    assert!(validate_disk_space(100, 99).is_err());
    assert!(validate_disk_space(100, 100).is_ok());
    assert!((1..=4).contains(&default_export_concurrency()));
}
