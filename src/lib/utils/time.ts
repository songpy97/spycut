export function formatTime(microseconds: number, showMillis = false): string {
  const safe = Math.max(0, Math.round(microseconds));
  const totalMilliseconds = Math.floor(safe / 1000);
  const milliseconds = totalMilliseconds % 1000;
  const totalSeconds = Math.floor(totalMilliseconds / 1000);
  const seconds = totalSeconds % 60;
  const totalMinutes = Math.floor(totalSeconds / 60);
  const minutes = totalMinutes % 60;
  const hours = Math.floor(totalMinutes / 60);
  const core = `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  return showMillis ? `${core}.${String(milliseconds).padStart(3, "0")}` : core;
}

export function formatDurationCompact(microseconds: number): string {
  const totalSeconds = Math.max(0, Math.round(microseconds / 1_000_000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) return `${hours}小时 ${minutes}分`;
  if (minutes > 0) return `${minutes}分 ${seconds}秒`;
  return `${seconds}秒`;
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** index;
  return `${value.toFixed(index < 2 ? 0 : 1)} ${units[index]}`;
}

export function clampTime(value: number, durationUs: number): number {
  return Math.min(Math.max(0, value), durationUs);
}

