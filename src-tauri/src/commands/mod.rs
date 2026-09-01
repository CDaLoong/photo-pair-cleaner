//! 全部 Tauri 命令，按前端模块分组。
//!
//! 命令层只负责参数解包、调用领域模块、把结果整理成前端契约，
//! 不承载业务规则——规则属于 pair_scan / pair_cleanup / file_organizer 等领域模块。

pub(crate) mod organizer;
pub(crate) mod preview;
pub(crate) mod rating;
pub(crate) mod scan;
pub(crate) mod system;
