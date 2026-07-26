import { writable } from 'svelte/store';
import type {
  FileRecord,
  ReviewItem,
  DriveInfo,
  MoveStatus,
  Config,
  Job,
} from '$lib/types';

// ── Connection ───────────────────────────────────────────────

export const connected = writable(false);
export const error = writable<string | null>(null);

// ── Files ────────────────────────────────────────────────────

export const files = writable<FileRecord[]>([]);
export const filesTotal = writable(0);
export const filesLoading = writable(false);

// ── Review ───────────────────────────────────────────────────

export const reviewQueue = writable<ReviewItem[]>([]);
export const reviewLoading = writable(false);

// ── Drives ───────────────────────────────────────────────────

export const drives = writable<DriveInfo[]>([]);
export const drivesLoading = writable(false);

// ── Move ─────────────────────────────────────────────────────

export const moveStatus = writable<MoveStatus>({
  confirmed_count: 0,
  moved_count: 0,
});

// ── Config ───────────────────────────────────────────────────

export const config = writable<Config | null>(null);

// ── Jobs ─────────────────────────────────────────────────────

export const jobs = writable<Job[]>([]);

// ── SSE ──────────────────────────────────────────────────────

export const lastEvent = writable<string | null>(null);
