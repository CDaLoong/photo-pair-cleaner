import type { AutoSyncOutcome } from "../rating-sync/types";

export type PreviewFilter = "all" | "paired" | "jpeg" | "raw";
export type PreviewSort = "name" | "modified" | "size";
export type PreviewView = "grid" | "loupe";

export type PhotoMemberKind = "jpeg" | "raw" | "xmp";

export interface PhotoMemberSnapshot {
  kind: PhotoMemberKind;
  relativePath: string;
  sizeBytes: number;
  modifiedMs: number | null;
}

export interface RatingState {
  framePair: number;
  jpegMetadata: number | null;
  rawXmp: number | null;
  resolved: number;
  conflict: boolean;
}

export interface PhotoAsset {
  id: string;
  name: string;
  relativeStem: string;
  previewPath: string | null;
  jpegPaths: string[];
  rawPaths: string[];
  xmpPaths: string[];
  members: PhotoMemberSnapshot[];
  extensions: string[];
  sizeBytes: number;
  modifiedMs: number | null;
  rating: number;
  ratingState: RatingState;
  ratingIssues: string[];
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

export interface PhotoDirectoryNode {
  name: string;
  path: string;
  directCount: number;
  totalCount: number;
  children: PhotoDirectoryNode[];
}

export interface RatingUpdate {
  assetId: string;
  rating: number;
  autoSync: AutoSyncOutcome;
}

export interface ExternalEditor {
  id: string;
  label: string;
  kind: "system" | "photoshop" | "lightroomClassic";
}
