import type {
  EmbeddedTemplateResource,
  FrameInsets,
  NormalizedPlacement,
  PhotoPlacementOverride,
  PhotoStyle,
  VariantLayerLayout,
  WatermarkBackground,
  WatermarkLayer,
  WatermarkOrientation,
  WatermarkTemplate,
} from "./types";

const HISTORY_LIMIT = 100;
const ORIENTATIONS: WatermarkOrientation[] = ["landscape", "portrait", "square"];

export interface WatermarkEditorDocument {
  template: WatermarkTemplate;
  photoOverrides: Record<string, PhotoPlacementOverride>;
  dirtyTemplate: boolean;
  unexportedChanges: boolean;
}

export interface WatermarkEditorState {
  past: WatermarkEditorDocument[];
  present: WatermarkEditorDocument;
  future: WatermarkEditorDocument[];
  activeLayerId: string | null;
  activeOrientation: WatermarkOrientation;
  historyGroup: string | null;
}

type HistoryGroup = string | null | undefined;

export type WatermarkEditorAction =
  | { type: "addLayer"; layer: WatermarkLayer; layouts: Record<WatermarkOrientation, VariantLayerLayout> }
  | { type: "updateLayer"; layerId: string; patch: Partial<WatermarkLayer>; historyGroup: HistoryGroup }
  | { type: "duplicateLayer"; layerId: string; newLayerId: string }
  | { type: "deleteLayer"; layerId: string }
  | { type: "reorderLayer"; layerId: string; toIndex: number }
  | { type: "setLayerLocked"; layerId: string; locked: boolean }
  | { type: "setLayerVisible"; layerId: string; visible: boolean }
  | { type: "setVariantFrame"; orientation: WatermarkOrientation; patch: Partial<FrameInsets>; historyGroup?: HistoryGroup }
  | { type: "setVariantPhoto"; orientation: WatermarkOrientation; patch: Partial<PhotoStyle>; historyGroup?: HistoryGroup }
  | { type: "setVariantBackground"; orientation: WatermarkOrientation; background: WatermarkBackground }
  | { type: "setCanvasRatio"; orientation: WatermarkOrientation; canvasRatio: number | null }
  | { type: "addResource"; resource: EmbeddedTemplateResource }
  | { type: "removeResource"; resourceId: string }
  | {
      type: "setLayerPlacement";
      orientation: WatermarkOrientation;
      layerId: string;
      patch: Partial<NormalizedPlacement>;
      historyGroup: HistoryGroup;
    }
  | {
      type: "setLayerFontSize";
      orientation: WatermarkOrientation;
      layerId: string;
      fontSizeRatio: number;
      historyGroup: HistoryGroup;
    }
  | { type: "setActiveLayer"; layerId: string | null }
  | { type: "setActiveOrientation"; orientation: WatermarkOrientation }
  | {
      type: "setPhotoOverride";
      photoId: string;
      patch: Partial<PhotoPlacementOverride>;
      historyGroup: HistoryGroup;
    }
  | { type: "clearPhotoOverride"; photoId: string }
  | { type: "closeHistoryGroup" }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "replaceTemplate"; template: WatermarkTemplate }
  | { type: "hydrateTemplate"; template: WatermarkTemplate }
  | { type: "markTemplateSaved" }
  | { type: "markExported" }
  | { type: "markSourceChanged" }
  | { type: "resetEditor"; template: WatermarkTemplate };

function clone<T>(value: T): T {
  return structuredClone(value);
}

function documentFor(template: WatermarkTemplate): WatermarkEditorDocument {
  return {
    template: clone(template),
    photoOverrides: {},
    dirtyTemplate: false,
    unexportedChanges: false,
  };
}

export function createWatermarkEditorState(template: WatermarkTemplate): WatermarkEditorState {
  return {
    past: [],
    present: documentFor(template),
    future: [],
    activeLayerId: template.shared.layers[0]?.id ?? null,
    activeOrientation: "landscape",
    historyGroup: null,
  };
}

function valuesDiffer<T extends object>(target: T, patch: Partial<T>): boolean {
  return Object.entries(patch).some(([key, value]) => (
    target[key as keyof T] !== value
  ));
}

function commit(
  state: WatermarkEditorState,
  present: WatermarkEditorDocument,
  templateChanged: boolean,
  historyGroup: HistoryGroup = null,
): WatermarkEditorState {
  present.dirtyTemplate = state.present.dirtyTemplate || templateChanged;
  present.unexportedChanges = true;
  const continuingGroup = Boolean(historyGroup) && state.historyGroup === historyGroup;
  const past = continuingGroup
    ? state.past
    : [...state.past, state.present].slice(-HISTORY_LIMIT);
  return {
    ...state,
    past,
    present,
    future: [],
    historyGroup: historyGroup ?? null,
  };
}

function editDocument(
  state: WatermarkEditorState,
  templateChanged: boolean,
  historyGroup: HistoryGroup,
  edit: (document: WatermarkEditorDocument) => boolean,
): WatermarkEditorState {
  const present = clone(state.present);
  return edit(present) ? commit(state, present, templateChanged, historyGroup) : state;
}

function findLayer(template: WatermarkTemplate, layerId: string): WatermarkLayer | undefined {
  return template.shared.layers.find((layer) => layer.id === layerId);
}

function normalizeLayerOrder(template: WatermarkTemplate): void {
  template.shared.layers.forEach((layer, index) => { layer.zIndex = index; });
}

function setLayerBoolean(
  state: WatermarkEditorState,
  layerId: string,
  key: "locked" | "visible",
  value: boolean,
): WatermarkEditorState {
  return editDocument(state, true, null, (document) => {
    const layer = findLayer(document.template, layerId);
    if (!layer || layer[key] === value) return false;
    layer[key] = value;
    return true;
  });
}

export function watermarkEditorReducer(
  state: WatermarkEditorState,
  action: WatermarkEditorAction,
): WatermarkEditorState {
  switch (action.type) {
    case "addLayer": {
      if (findLayer(state.present.template, action.layer.id)) return state;
      const next = editDocument(state, true, null, (document) => {
        document.template.shared.layers.push(clone(action.layer));
        for (const orientation of ORIENTATIONS) {
          document.template.variants[orientation].layerLayouts[action.layer.id] = clone(action.layouts[orientation]);
        }
        normalizeLayerOrder(document.template);
        return true;
      });
      return { ...next, activeLayerId: action.layer.id };
    }
    case "updateLayer":
      return editDocument(state, true, action.historyGroup, (document) => {
        const layer = findLayer(document.template, action.layerId);
        if (!layer || !valuesDiffer(layer, action.patch)) return false;
        Object.assign(layer, action.patch);
        return true;
      });
    case "duplicateLayer": {
      if (findLayer(state.present.template, action.newLayerId)) return state;
      const sourceIndex = state.present.template.shared.layers.findIndex((layer) => layer.id === action.layerId);
      if (sourceIndex < 0) return state;
      const next = editDocument(state, true, null, (document) => {
        const copy = clone(document.template.shared.layers[sourceIndex]);
        copy.id = action.newLayerId;
        copy.name = `${copy.name} 副本`;
        document.template.shared.layers.splice(sourceIndex + 1, 0, copy);
        for (const orientation of ORIENTATIONS) {
          const sourceLayout = document.template.variants[orientation].layerLayouts[action.layerId];
          if (!sourceLayout) return false;
          document.template.variants[orientation].layerLayouts[action.newLayerId] = clone(sourceLayout);
        }
        normalizeLayerOrder(document.template);
        return true;
      });
      return { ...next, activeLayerId: action.newLayerId };
    }
    case "deleteLayer": {
      const sourceIndex = state.present.template.shared.layers.findIndex((layer) => layer.id === action.layerId);
      if (sourceIndex < 0) return state;
      const next = editDocument(state, true, null, (document) => {
        document.template.shared.layers.splice(sourceIndex, 1);
        for (const orientation of ORIENTATIONS) {
          delete document.template.variants[orientation].layerLayouts[action.layerId];
        }
        normalizeLayerOrder(document.template);
        return true;
      });
      const layers = next.present.template.shared.layers;
      return {
        ...next,
        activeLayerId: state.activeLayerId === action.layerId
          ? layers[Math.min(sourceIndex, layers.length - 1)]?.id ?? null
          : state.activeLayerId,
      };
    }
    case "reorderLayer":
      return editDocument(state, true, null, (document) => {
        const layers = document.template.shared.layers;
        const fromIndex = layers.findIndex((layer) => layer.id === action.layerId);
        const toIndex = Math.max(0, Math.min(layers.length - 1, action.toIndex));
        if (fromIndex < 0 || fromIndex === toIndex) return false;
        const [layer] = layers.splice(fromIndex, 1);
        layers.splice(toIndex, 0, layer);
        normalizeLayerOrder(document.template);
        return true;
      });
    case "setLayerLocked":
      return setLayerBoolean(state, action.layerId, "locked", action.locked);
    case "setLayerVisible":
      return setLayerBoolean(state, action.layerId, "visible", action.visible);
    case "setVariantFrame":
      return editDocument(state, true, action.historyGroup, (document) => {
        const frame = document.template.variants[action.orientation].frame;
        if (!valuesDiffer(frame, action.patch)) return false;
        Object.assign(frame, action.patch);
        return true;
      });
    case "setVariantPhoto":
      return editDocument(state, true, action.historyGroup, (document) => {
        const photo = document.template.variants[action.orientation].photo;
        if (!valuesDiffer(photo, action.patch)) return false;
        Object.assign(photo, action.patch);
        return true;
      });
    case "setVariantBackground":
      return editDocument(state, true, null, (document) => {
        document.template.variants[action.orientation].background = clone(action.background);
        return true;
      });
    case "setCanvasRatio":
      return editDocument(state, true, null, (document) => {
        const variant = document.template.variants[action.orientation];
        if (variant.canvasRatio === action.canvasRatio) return false;
        variant.canvasRatio = action.canvasRatio;
        return true;
      });
    case "addResource":
      return editDocument(state, true, null, (document) => {
        if (document.template.resources[action.resource.id]) return false;
        document.template.resources[action.resource.id] = clone(action.resource);
        return true;
      });
    case "removeResource":
      return editDocument(state, true, null, (document) => {
        if (!document.template.resources[action.resourceId]) return false;
        if (document.template.shared.layers.some((layer) => (
          layer.kind === "image" && layer.resourceId === action.resourceId
        ))) return false;
        delete document.template.resources[action.resourceId];
        return true;
      });
    case "setLayerPlacement":
      return editDocument(state, true, action.historyGroup, (document) => {
        const layout = document.template.variants[action.orientation].layerLayouts[action.layerId];
        if (!layout || !valuesDiffer(layout.placement, action.patch)) return false;
        Object.assign(layout.placement, action.patch);
        return true;
      });
    case "setLayerFontSize":
      return editDocument(state, true, action.historyGroup, (document) => {
        const layout = document.template.variants[action.orientation].layerLayouts[action.layerId];
        if (!layout || layout.fontSizeRatio === action.fontSizeRatio) return false;
        layout.fontSizeRatio = action.fontSizeRatio;
        return true;
      });
    case "setActiveLayer":
      if (action.layerId && !findLayer(state.present.template, action.layerId)) return state;
      return state.activeLayerId === action.layerId ? state : { ...state, activeLayerId: action.layerId };
    case "setActiveOrientation":
      return state.activeOrientation === action.orientation
        ? state
        : { ...state, activeOrientation: action.orientation };
    case "setPhotoOverride":
      return editDocument(state, false, action.historyGroup, (document) => {
        const current = document.photoOverrides[action.photoId] ?? { alignX: 0.5, alignY: 0.5, scale: 1 };
        if (!valuesDiffer(current, action.patch)) return false;
        document.photoOverrides[action.photoId] = { ...current, ...action.patch };
        return true;
      });
    case "clearPhotoOverride":
      return editDocument(state, false, null, (document) => {
        if (!document.photoOverrides[action.photoId]) return false;
        delete document.photoOverrides[action.photoId];
        return true;
      });
    case "closeHistoryGroup":
      return state.historyGroup ? { ...state, historyGroup: null } : state;
    case "undo": {
      if (state.past.length === 0) return state;
      const present = state.past.at(-1)!;
      return {
        ...state,
        past: state.past.slice(0, -1),
        present,
        future: [state.present, ...state.future].slice(0, HISTORY_LIMIT),
        historyGroup: null,
      };
    }
    case "redo": {
      if (state.future.length === 0) return state;
      const [present, ...future] = state.future;
      return {
        ...state,
        past: [...state.past, state.present].slice(-HISTORY_LIMIT),
        present,
        future,
        historyGroup: null,
      };
    }
    case "replaceTemplate":
      return {
        ...createWatermarkEditorState(action.template),
        present: {
          ...documentFor(action.template),
          unexportedChanges: true,
        },
      };
    case "hydrateTemplate": {
      const next = createWatermarkEditorState(action.template);
      return {
        ...next,
        present: {
          ...next.present,
          unexportedChanges: state.present.unexportedChanges,
        },
      };
    }
    case "markTemplateSaved":
      return state.present.dirtyTemplate
        ? { ...state, present: { ...state.present, dirtyTemplate: false } }
        : state;
    case "markExported":
      return state.present.unexportedChanges
        ? { ...state, present: { ...state.present, unexportedChanges: false } }
        : state;
    case "markSourceChanged":
      return state.present.unexportedChanges
        ? state
        : { ...state, present: { ...state.present, unexportedChanges: true } };
    case "resetEditor":
      return createWatermarkEditorState(action.template);
  }
}
