/**
 * 应用写入 localStorage 的全部键，集中在一处。
 *
 * 集中的理由：这就是「应用会记住哪些东西」的完整清单。散落在各组件里时，
 * 曾经出现过两个模块各自定义 `ROOT_STORAGE_KEY` 却指向不同值的情况，
 * 从调用处根本看不出来它们不是同一个键。
 *
 * 命名约定：`framepair.<模块>.<用途>.v<N>`。
 * 已持久化数据的结构一旦发生不兼容变更，请递增 `vN` 而不是原地改语义——
 * 老用户机器上的旧值还在，读到不认识的结构要能安全地退回默认值。
 */
export const STORAGE_KEYS = {
  /** 成对清理模块的目录与选项。 */
  cleanupSettings: "framepair.settings.v2",
  /** 成对清理的新手引导是否已看过。 */
  cleanupGuideCompleted: "framepair.guide.completed.v1",

  /** 照片浏览模块上次打开的目录。 */
  previewRoot: "framepair.preview.root.v1",
  /** 照片浏览的新手引导是否已看过。 */
  previewGuide: "framepair.preview.guide.v1",
  /** 照片浏览左侧文件夹栏是否收起。 */
  previewFolderSidebarCollapsed: "framepair.preview.folder-sidebar-collapsed.v1",

  /** 水印导出左侧面板是否收起。 */
  watermarkLeftPanelCollapsed: "framepair.watermark.left-panel-collapsed.v1",
  /** 水印导出右侧面板是否收起。 */
  watermarkRightPanelCollapsed: "framepair.watermark.right-panel-collapsed.v1",
  /** 水印导出的新手引导是否已看过。 */
  watermarkGuide: "framepair.watermark.guide.v1",

  /** 评分同步子任务上次使用的照片目录。 */
  ratingSyncRoot: "framepair.rating-sync.root.v1",
  /** 评分整理子任务上次使用的照片目录。 */
  ratingRulesRoot: "framepair.rating-rules.root.v1",

  /** 左侧模块导航栏是否收起。 */
  moduleSidebarCollapsed: "framepair.layout.module-sidebar-collapsed.v1",
} as const;

export type StorageKey = (typeof STORAGE_KEYS)[keyof typeof STORAGE_KEYS];
