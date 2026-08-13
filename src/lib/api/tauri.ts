import { invoke, isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ActiveExportProjection, AudioWaveform, CommandFailure, DiagnosticStatus, ExportProgress, ExportResult,
  ExportStarted, OpenSourceResult, PlaybackDiagnostic, RecoverableExport, SessionProjection
} from "../types/contracts";

export function runningInTauri(): boolean {
  return isTauri();
}

export async function chooseSource(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    directory: false,
    title: "选择直播录屏",
    filters: [{ name: "MP4 录屏", extensions: ["mp4"] }]
  });
  return typeof selected === "string" ? selected : null;
}

export async function chooseDestination(defaultPath?: string): Promise<string | null> {
  return save({
    title: "导出公开课程视频",
    defaultPath,
    filters: [{ name: "MP4 视频", extensions: ["mp4"] }]
  });
}

export async function openSource(path: string): Promise<OpenSourceResult> {
  return invoke<OpenSourceResult>("open_source", { path });
}

export async function getSession(): Promise<OpenSourceResult | null> {
  return invoke<OpenSourceResult | null>("get_session");
}

export async function getLaunchSource(): Promise<string | null> {
  return invoke<string | null>("get_launch_source");
}

export async function diagnosePlayback(): Promise<PlaybackDiagnostic> {
  return invoke<PlaybackDiagnostic>("diagnose_playback");
}

export async function getAudioWaveform(projectId: string): Promise<AudioWaveform> {
  return invoke<AudioWaveform>("get_audio_waveform", { projectId });
}

export async function getDiagnosticStatus(): Promise<DiagnosticStatus> {
  return invoke<DiagnosticStatus>("get_diagnostic_status");
}

export async function recordFrontendDiagnostic(
  kind: "frontend_ready" | "frontend_error" | "unhandled_rejection" | "player_error",
  message: string
): Promise<void> {
  return invoke<void>("record_frontend_diagnostic", { kind, message });
}

export async function revealDiagnosticLog(): Promise<void> {
  const status = await getDiagnosticStatus();
  if (!status.available) throw new Error("诊断日志当前不可用，请检查应用数据目录的写入权限");
  return revealItemInDir(status.logPath);
}

export async function addDeleteInterval(startUs: number, endUs: number, projectId: string): Promise<SessionProjection> {
  return invoke<SessionProjection>("add_delete_interval", { startUs, endUs, projectId });
}

export async function resizeDeleteInterval(
  id: number,
  startUs: number,
  endUs: number,
  projectId: string
): Promise<SessionProjection> {
  return invoke<SessionProjection>("resize_delete_interval", { id, startUs, endUs, projectId });
}

export async function removeDeleteInterval(id: number, projectId: string): Promise<SessionProjection> {
  return invoke<SessionProjection>("remove_delete_interval", { id, projectId });
}

export async function setPlayhead(playheadUs: number, projectId: string): Promise<SessionProjection> {
  return invoke<SessionProjection>("set_playhead", { playheadUs, projectId });
}

export async function setJoinReviewed(id: number, reviewed: boolean, projectId: string): Promise<SessionProjection> {
  return invoke<SessionProjection>("set_join_reviewed", { id, reviewed, projectId });
}

export async function undoEdit(projectId: string): Promise<SessionProjection> {
  return invoke<SessionProjection>("undo", { projectId });
}

export async function redoEdit(projectId: string): Promise<SessionProjection> {
  return invoke<SessionProjection>("redo", { projectId });
}

export async function startExport(
  destination: string,
  allowUnreviewed: boolean,
  allowBitDepthFallback: boolean
): Promise<ExportStarted> {
  return invoke<ExportStarted>("start_export", { destination, allowUnreviewed, allowBitDepthFallback });
}

export async function cancelExport(jobId: string): Promise<void> {
  return invoke<void>("cancel_export", { jobId });
}

export async function getActiveExport(): Promise<ActiveExportProjection | null> {
  return invoke<ActiveExportProjection | null>("get_active_export");
}

export async function listRecoverableExports(): Promise<RecoverableExport[]> {
  return invoke<RecoverableExport[]>("list_recoverable_exports");
}

export async function cleanupRecoverableExport(jobId: string): Promise<void> {
  return invoke<void>("cleanup_recoverable_export", { jobId });
}

export function onExportProgress(handler: (event: ExportProgress) => void): Promise<UnlistenFn> {
  return listen<ExportProgress>("spycut://export-progress", (event) => handler(event.payload));
}

export function onExportResult(handler: (event: ExportResult) => void): Promise<UnlistenFn> {
  return listen<ExportResult>("spycut://export-result", (event) => handler(event.payload));
}

export async function revealOutput(path: string): Promise<void> {
  return revealItemInDir(path);
}

export function commandMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object") {
    const candidate = error as Partial<CommandFailure>;
    if (candidate.message) return candidate.message;
  }
  return "操作没有完成，请查看诊断信息后重试。";
}
