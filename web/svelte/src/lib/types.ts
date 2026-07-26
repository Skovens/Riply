// ── Pipeline Status ──────────────────────────────────────────

export type PipelineStatus =
  | 'Scanned'
  | 'Matched'
  | 'NeedsReview'
  | 'Confirmed'
  | 'Moved'
  | 'Error';

// ── Media Types ──────────────────────────────────────────────

export type MediaType = 'Movie' | 'Tv';

// ── File Record ──────────────────────────────────────────────

export interface FileRecord {
  id: string;
  path: string;
  filename: string;
  size?: number;
  duration?: number;
  hash: string;
  status: PipelineStatus;
  suggested_title?: string;
  suggested_year?: number;
  confidence?: number;
  canonical_title?: string;
  canonical_year?: number;
  tmdb_id?: number;
  content_type: string;
  media_type: MediaType;
  series_name?: string;
  season_number?: number;
  episode_number?: number;
  episode_title?: string;
  source?: string;
  subtitle_text?: string;
  duplicate_path?: string;
  reviewed: boolean;
  created_at: string;
  updated_at: string;
}

// ── Review Item ──────────────────────────────────────────────

export interface ReviewItem {
  id: string;
  filename: string;
  path: string;
  suggested_title?: string;
  canonical_title?: string;
  confidence?: number;
  media_type: MediaType;
  series_name?: string;
  season_number?: number;
  episode_number?: number;
}

// ── Drive Info ───────────────────────────────────────────────

export type DriveState = 'Empty' | 'Loading' | 'Ready' | 'Ripping' | 'Complete' | 'Error';

export interface DriveInfo {
  index: number;
  name: string;
  disc_name?: string;
  state: DriveState;
}

// ── Move Status ──────────────────────────────────────────────

export interface MoveStatus {
  confirmed_count: number;
  moved_count: number;
}

// ── Config ───────────────────────────────────────────────────

export interface RippingConfig {
  command?: string;
  makemkvcon_path?: string;
}

export interface Config {
  database_path: string;
  scan_directory?: string;
  movies_directory: string;
  tv_directory: string;
  verbose: boolean;
  tmdb_api_key?: string;
  omdb_api_key?: string;
  makemkvcon_path?: string;
  ripping: RippingConfig;
  host: string;
  port: number;
}

// ── Job ──────────────────────────────────────────────────────

export interface Job {
  id: string;
  type: string;
  status: string;
  progress?: number;
  created_at: string;
  completed_at?: string;
}

// ── API Response Wrappers ────────────────────────────────────

export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

export interface PaginatedResponse<T> {
  items: T[];
  total: number;
  page: number;
  per_page: number;
}
