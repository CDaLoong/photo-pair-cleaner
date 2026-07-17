export type PreviewFilter = "all" | "paired" | "jpeg" | "raw";
export type PreviewSort = "name" | "modified" | "size";
export type PreviewView = "grid" | "loupe";

export interface PhotoAsset {
  id: string;
  name: string;
  relativeStem: string;
  previewPath: string | null;
  jpegPaths: string[];
  rawPaths: string[];
  extensions: string[];
  sizeBytes: number;
  modifiedMs: number | null;
  rating: number;
}

export interface PhotoIndex {
  root: string;
  indexedAtMs: number;
  totalAssets: number;
  pairedAssets: number;
  previewableAssets: number;
  rawOnlyAssets: number;
  assets: PhotoAsset[];
}

export interface RatingUpdate {
  assetId: string;
  rating: number;
}

export interface ExternalEditor {
  id: string;
  label: string;
  kind: "system" | "photoshop" | "lightroomClassic";
}
