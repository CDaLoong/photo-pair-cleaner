//! FramePair 后端入口：声明模块、托管进程内状态、注册全部 Tauri 命令。
//!
//! 分层约定（新增代码请按此归位）：
//! - `commands/`  只做参数解包与结果整形，不放业务规则；
//! - 领域模块（`pair_scan`、`pair_cleanup`、`file_organizer`、`rating_sync` 等）
//!   承载规则与不变量；
//! - `fs_util`    收敛所有路径安全校验与时间戳工具；
//! - `app_state`  存放「一次性计划」这类被 Tauri 托管的状态。

mod app_paths;
mod app_state;
mod commands;
mod editors;
mod file_organizer;
mod formats;
mod fs_util;
#[cfg(target_os = "macos")]
mod native_preview;
mod operation_history;
mod operation_plan;
mod pair_cleanup;
mod pair_scan;
mod photo_groups;
mod platform;
mod preview;
mod preview_cache;
mod quarantine;
mod rating_metadata;
mod rating_rules;
mod rating_sync;
mod ratings;
mod reference;
mod safety;
mod watermark_color;
mod watermark_commands;
mod watermark_export;
mod watermark_geometry;
mod watermark_metadata;
mod watermark_model;
mod watermark_output;
mod watermark_render;
mod watermark_resource;
mod watermark_source;
mod watermark_templates;
mod watermark_text;
#[cfg(windows)]
mod windows_thumbnail;

use crate::app_state::{RatingStore, ScanPlanStore};
use crate::commands::{organizer, preview as preview_commands, rating, scan, system};
use watermark_commands::{
    WatermarkRenderState, acknowledge_watermark_export, cancel_watermark_export,
    delete_watermark_template, export_watermark_template, import_watermark_resource,
    import_watermark_template, list_watermark_fonts, list_watermark_templates,
    prepare_watermark_source, render_watermark_preview, retry_watermark_export_failures,
    reveal_watermark_export, save_watermark_template, start_watermark_export,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ScanPlanStore::default())
        .manage(RatingStore::default())
        .manage(rating_sync::RatingSyncPlanStore::default())
        .manage(operation_plan::OperationPlanStore::default())
        .manage(WatermarkRenderState::default())
        .manage(watermark_export::WatermarkExportStore::default())
        .invoke_handler(tauri::generate_handler![
            scan::validate_directory_path,
            scan::scan_pairs,
            scan::execute_cleanup,
            scan::export_audit_manifest,
            scan::list_quarantine_operations,
            scan::restore_quarantine_operation,
            scan::reveal_quarantine_operation,
            scan::reveal_scan_item,
            rating::index_photo_directory,
            rating::set_photo_rating,
            rating::get_rating_sync_state,
            rating::save_rating_sync_settings,
            rating::generate_rating_sync_plan,
            rating::execute_rating_sync_plan,
            rating::get_rating_rules,
            rating::save_rating_rules,
            rating::import_rating_rules,
            rating::export_rating_rules,
            organizer::generate_operation_plan,
            organizer::execute_operation_plan,
            organizer::list_rating_operation_history,
            organizer::restore_rating_move,
            organizer::restore_rating_quarantine,
            organizer::undo_rating_copy,
            preview_commands::load_photo_thumbnail,
            preview_commands::prepare_photo_original,
            preview_commands::get_preview_cache_stats,
            preview_commands::show_native_photo_preview,
            preview_commands::hide_native_photo_preview,
            system::list_external_editors,
            system::open_photo_in_editor,
            prepare_watermark_source,
            list_watermark_fonts,
            import_watermark_resource,
            list_watermark_templates,
            save_watermark_template,
            delete_watermark_template,
            import_watermark_template,
            export_watermark_template,
            render_watermark_preview,
            start_watermark_export,
            cancel_watermark_export,
            retry_watermark_export_failures,
            reveal_watermark_export,
            acknowledge_watermark_export,
            system::reveal_operation_log,
            system::open_system_trash
        ])
        .run(tauri::generate_context!())
        .expect("failed to run FramePair");
}

#[cfg(test)]
mod tests {
    /// 校验命令注册表本身：整理类命令必须全部暴露，
    /// 且不得回退到那个接受任意路径的旧命令 execute_rating_cleanup。
    #[test]
    fn frontend_exposes_rating_organizer_execution() {
        let _ = crate::commands::organizer::execute_operation_plan;
        let _ = crate::commands::organizer::list_rating_operation_history;
        let _ = crate::commands::organizer::restore_rating_move;
        let _ = crate::commands::organizer::undo_rating_copy;

        let source = include_str!("lib.rs");
        let handler = source
            .split("tauri::generate_handler![")
            .nth(1)
            .and_then(|value| value.split("])").next())
            .expect("Tauri command registration");
        assert!(!handler.contains("execute_rating_cleanup"));
    }
}
