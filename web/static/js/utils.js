/**
 * Riply Shared Utilities
 *
 * Common helper functions used across all components.
 */

/**
 * Escape HTML special characters to prevent XSS.
 */
export function escapeHtml(str) {
  if (!str) return '';
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

/**
 * Format bytes to human-readable size string.
 */
export function formatSize(bytes) {
  if (!bytes || bytes === 0) return '-';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  let size = bytes;
  while (size >= 1024 && i < units.length - 1) {
    size /= 1024;
    i++;
  }
  return `${size.toFixed(i > 0 ? 1 : 0)} ${units[i]}`;
}

/**
 * Map pipeline status to CSS badge color class.
 */
export function statusColor(status) {
  const colors = {
    Scanned: 'default',
    Matched: 'accent',
    'Needs Review': 'warning',
    Confirmed: 'success',
    Moved: 'accent',
    Failed: 'error',
  };
  return colors[status] || 'default';
}
