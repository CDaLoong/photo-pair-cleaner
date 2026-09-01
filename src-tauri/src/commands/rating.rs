//! 照片浏览与评分模块的命令：建立索引、写评分、评分同步、评分规则。

use crate::app_state::{PhotoRatingUpdate, RatingStore, next_plan_id};
use crate::fs_util::now_ms;
use crate::{photo_groups, rating_rules, rating_sync, ratings};
use std::path::Path;
use std::sync::Arc;
use tauri::Manager;

#[tauri::command]
pub(crate) async fn index_photo_directory(
    app: tauri::AppHandle,
    state: tauri::State<'_, RatingStore>,
    root: String,
    on_event: tauri::ipc::Channel<photo_groups::PhotoIndexEvent>,
) -> Result<photo_groups::PhotoIndex, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分数据目录：{error}"))?
        .join("photo-ratings.json");
    let access = Arc::clone(&state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分数据库".to_string())?;
        let progress_channel = on_event.clone();
        let mut index = photo_groups::index_directory_with_events(Path::new(&root), |event| {
            let _ = progress_channel.send(event);
        })?;
        let _ = on_event.send(photo_groups::PhotoIndexEvent::Progress {
            progress: photo_groups::PhotoIndexProgress {
                phase: photo_groups::PhotoIndexPhase::Finalizing,
                completed: 0,
                total: Some(1),
                files_found: index.assets.iter().map(|asset| asset.members.len()).sum(),
                assets_found: index.total_assets,
            },
        });
        let ratings = ratings::load_ratings(&database_path, Path::new(&root))?;
        photo_groups::apply_framepair_ratings(&mut index, &ratings);
        let _ = on_event.send(photo_groups::PhotoIndexEvent::Progress {
            progress: photo_groups::PhotoIndexProgress {
                phase: photo_groups::PhotoIndexPhase::Finalizing,
                completed: 1,
                total: Some(1),
                files_found: index.assets.iter().map(|asset| asset.members.len()).sum(),
                assets_found: index.total_assets,
            },
        });
        Ok(index)
    })
    .await
    .map_err(|error| format!("照片索引任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn set_photo_rating(
    app: tauri::AppHandle,
    state: tauri::State<'_, RatingStore>,
    root: String,
    relative_path: String,
    rating: u8,
) -> Result<PhotoRatingUpdate, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分数据目录：{error}"))?
        .join("photo-ratings.json");
    let sync_database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分同步设置目录：{error}"))?
        .join("rating-sync.json");
    let access = Arc::clone(&state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分数据库".to_string())?;
        let update = ratings::set_rating(&database_path, Path::new(&root), &relative_path, rating)?;
        let sync_state = rating_sync::load_sync_state(&sync_database_path, Some(Path::new(&root)));
        let auto_sync = match sync_state {
            Ok(sync_state) if sync_state.settings.mode == rating_sync::RatingSyncMode::Manual => {
                rating_sync::AutoSyncOutcome {
                    status: rating_sync::AutoSyncStatus::Disabled,
                    message: None,
                }
            }
            Ok(sync_state) => {
                let index =
                    photo_groups::index_directory(Path::new(&root)).and_then(|mut index| {
                        let saved = ratings::load_ratings(&database_path, Path::new(&root))?;
                        photo_groups::apply_framepair_ratings(&mut index, &saved);
                        Ok(index)
                    });
                match index {
                    Ok(index) => rating_sync::auto_sync_saved_rating(
                        &sync_database_path,
                        &index,
                        &sync_state.settings,
                        Path::new(&root),
                        &update.asset_id,
                        update.rating,
                        &next_plan_id(),
                        now_ms(),
                    ),
                    Err(error) => {
                        let pending = rating_sync::PendingRatingSync {
                            root: root.clone(),
                            asset_id: update.asset_id.clone(),
                            rating: update.rating,
                            targets: sync_state.settings.targets,
                            error: error.clone(),
                            failed_at_ms: now_ms(),
                        };
                        let message = rating_sync::record_pending(&sync_database_path, pending)
                            .err()
                            .map(|pending_error| {
                                format!("{error}；待处理状态保存失败：{pending_error}")
                            })
                            .unwrap_or(error);
                        rating_sync::AutoSyncOutcome {
                            status: rating_sync::AutoSyncStatus::Pending,
                            message: Some(message),
                        }
                    }
                }
            }
            Err(error) => rating_sync::AutoSyncOutcome {
                status: rating_sync::AutoSyncStatus::Pending,
                message: Some(format!(
                    "FramePair 评分已保存，但无法读取自动同步设置：{error}"
                )),
            },
        };
        Ok(PhotoRatingUpdate {
            asset_id: update.asset_id,
            rating: update.rating,
            auto_sync,
        })
    })
    .await
    .map_err(|error| format!("保存照片评分任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn get_rating_sync_state(
    app: tauri::AppHandle,
    state: tauri::State<'_, RatingStore>,
    root: Option<String>,
) -> Result<rating_sync::RatingSyncState, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分同步设置目录：{error}"))?
        .join("rating-sync.json");
    let access = Arc::clone(&state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分同步设置".to_string())?;
        rating_sync::load_sync_state(&database_path, root.as_deref().map(Path::new))
    })
    .await
    .map_err(|error| format!("读取评分同步设置任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn save_rating_sync_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, RatingStore>,
    settings: rating_sync::RatingSyncSettings,
) -> Result<rating_sync::RatingSyncSettings, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分同步设置目录：{error}"))?
        .join("rating-sync.json");
    let access = Arc::clone(&state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分同步设置".to_string())?;
        rating_sync::save_sync_settings(&database_path, &settings)
    })
    .await
    .map_err(|error| format!("保存评分同步设置任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn generate_rating_sync_plan(
    app: tauri::AppHandle,
    rating_state: tauri::State<'_, RatingStore>,
    plan_state: tauri::State<'_, rating_sync::RatingSyncPlanStore>,
    request: rating_sync::RatingSyncPlanRequest,
) -> Result<rating_sync::RatingSyncPlanSummary, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分数据目录：{error}"))?
        .join("photo-ratings.json");
    let access = Arc::clone(&rating_state.access);
    let plan_id = next_plan_id();
    let plan = tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分数据库".to_string())?;
        let mut index = photo_groups::index_directory(Path::new(&request.root))?;
        let ratings = ratings::load_ratings(&database_path, Path::new(&request.root))?;
        photo_groups::apply_framepair_ratings(&mut index, &ratings);
        rating_sync::build_plan(&index, &request, plan_id)
    })
    .await
    .map_err(|error| format!("评分同步计划任务异常结束：{error}"))??;
    let summary = plan.summary().clone();
    plan_state.replace(plan)?;
    Ok(summary)
}

#[tauri::command]
pub(crate) async fn execute_rating_sync_plan(
    rating_state: tauri::State<'_, RatingStore>,
    plan_state: tauri::State<'_, rating_sync::RatingSyncPlanStore>,
    request: rating_sync::RatingSyncExecuteRequest,
) -> Result<rating_sync::RatingSyncExecutionSummary, String> {
    let plan = plan_state.take(&request.plan_id, Path::new(&request.root))?;
    let access = Arc::clone(&rating_state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分同步任务".to_string())?;
        rating_sync::execute_plan(&plan, &request)
    })
    .await
    .map_err(|error| format!("评分同步执行任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn get_rating_rules(
    app: tauri::AppHandle,
    state: tauri::State<'_, RatingStore>,
) -> Result<rating_rules::RatingRuleState, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分规则目录：{error}"))?
        .join("rating-rules.json");
    let access = Arc::clone(&state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access.lock().map_err(|_| "无法锁定评分规则".to_string())?;
        rating_rules::load_rules(&database_path)
    })
    .await
    .map_err(|error| format!("读取评分规则任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn save_rating_rules(
    app: tauri::AppHandle,
    state: tauri::State<'_, RatingStore>,
    rules: Vec<rating_rules::RatingRule>,
) -> Result<rating_rules::RatingRuleState, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分规则目录：{error}"))?
        .join("rating-rules.json");
    let access = Arc::clone(&state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access.lock().map_err(|_| "无法锁定评分规则".to_string())?;
        rating_rules::save_rules(&database_path, &rules)
    })
    .await
    .map_err(|error| format!("保存评分规则任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn import_rating_rules(
    path: String,
) -> Result<rating_rules::RatingRuleState, String> {
    tauri::async_runtime::spawn_blocking(move || rating_rules::import_rules(Path::new(&path)))
        .await
        .map_err(|error| format!("导入评分规则任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn export_rating_rules(
    path: String,
    rules: Vec<rating_rules::RatingRule>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        rating_rules::export_rules(Path::new(&path), &rules)
    })
    .await
    .map_err(|error| format!("导出评分规则任务异常结束：{error}"))?
}
