<script lang="ts">
  import { onMount } from "svelte";
  import EmptyState from "./lib/components/EmptyState.svelte";
  import ErrorBanner from "./lib/components/ErrorBanner.svelte";
  import ExportPanel from "./lib/components/ExportPanel.svelte";
  import IntervalList from "./lib/components/IntervalList.svelte";
  import PlayerPane from "./lib/components/PlayerPane.svelte";
  import ReviewPanel from "./lib/components/ReviewPanel.svelte";
  import TransportControls from "./lib/components/TransportControls.svelte";
  import TimelineEditor from "./lib/components/TimelineEditor.svelte";
  import { DeletionPlaybackGuard } from "./lib/player/DeletionPlaybackGuard";
  import {
    addDeleteInterval, cancelExport, chooseDestination, chooseSource, cleanupRecoverableExport, commandMessage, diagnosePlayback, getAudioWaveform, getLaunchSource, getSession,
    listRecoverableExports,
    openSource, redoEdit, removeDeleteInterval, resizeDeleteInterval, runningInTauri,
    onExportProgress, onExportResult, recordFrontendDiagnostic, revealDiagnosticLog, revealOutput, setJoinReviewed, setPlayhead,
    startExport, undoEdit
  } from "./lib/api/tauri";
  import type { AudioWaveform, CommandFailure, DeleteInterval, ExportProgress, ExportResult, ExportStarted, ProjectV1, RecoverableExport, SessionProjection } from "./lib/types/contracts";
  import { createDemoSession } from "./lib/types/contracts";
  import { clampTime, formatBytes, formatDurationCompact, formatTime } from "./lib/utils/time";

  let currentSession: SessionProjection | null = null;
  let mediaSourceUrl = "";
  let loading = false;
  let demo = false;
  let errorMessage: string | null = null;
  let player: PlayerPane;
  let playheadUs = 0;
  let playing = false;
  let playbackRate = 1;
  let pendingStartUs: number | null = null;
  let markSaving = false;
  let selectedId: number | null = null;
  let scrubbing = false;
  let scrubOriginUs = 0;
  let resumeAfterScrub = false;
  let reviewOpen = false;
  let reviewIndex = 0;
  let reviewJump: { startUs: number; endUs: number; stopUs: number; jumped: boolean } | null = null;
  let exportOpen = false;
  let exportPreparing = false;
  let exportStarted: ExportStarted | null = null;
  let exportProgress: ExportProgress | null = null;
  let exportResult: ExportResult | null = null;
  let saveWarning: string | null = null;
  let pendingPlayheadSaveUs: number | null = null;
  let playheadSaveTimer: number | null = null;
  let playheadSaveInFlight = false;
  let lastPlayheadSaveAt = 0;
  let recoverableExports: RecoverableExport[] = [];
  let recoveryBusyJobId: string | null = null;
  let deletionSkipInFlight = false;
  let playbackSeekSequence = 0;
  let waveform: AudioWaveform | null = null;
  let waveformState: "loading" | "ready" | "unavailable" | "failed" = "unavailable";
  let waveformMessage = "";
  let waveformRequestSequence = 0;

  const PLAYHEAD_SAVE_INTERVAL_MS = 2_000;
  const deletionPlaybackGuard = new DeletionPlaybackGuard();
  const playbackSeekTokens = new Set<number>();

  $: project = currentSession?.project;
  $: media = project?.media;
  $: intervals = project?.deleteIntervals ?? [];
  $: reviewedIds = project?.reviewedIntervalIds ?? [];
  $: selectedInterval = intervals.find((item) => item.id === selectedId) ?? null;
  $: sourceName = project ? fileName(project.source.canonicalPath) : "";
  $: frameDurationUs = media ? Math.round((1_000_000 * media.frameRate.den) / media.frameRate.num) : 33_333;
  $: editLocked = exportOpen && !exportResult;

  onMount(() => {
    window.addEventListener("keydown", handleKeyboard);
    void restoreInitialSession();
    const tauriRuntime = runningInTauri();
    const handleWindowError = (event: ErrorEvent) => {
      void recordDiagnosticSafely("frontend_error", diagnosticText(event.error ?? event.message));
    };
    const handleUnhandledRejection = (event: PromiseRejectionEvent) => {
      void recordDiagnosticSafely("unhandled_rejection", diagnosticText(event.reason));
    };
    if (tauriRuntime) {
      window.addEventListener("error", handleWindowError);
      window.addEventListener("unhandledrejection", handleUnhandledRejection);
      void recordDiagnosticSafely("frontend_ready", `user_agent=${navigator.userAgent}`);
      void refreshRecoverableExports();
    }
    const unlisteners: Array<() => void> = [];
    let disposed = false;
    if (tauriRuntime) {
      void onExportProgress((event) => {
        if (exportStarted && event.jobId !== exportStarted.jobId) return;
        exportProgress = event;
        exportOpen = true;
      }).then((unlisten) => disposed ? unlisten() : unlisteners.push(unlisten));
      void onExportResult((event) => {
        if (exportStarted && event.jobId !== exportStarted.jobId) return;
        exportResult = event;
        exportPreparing = false;
        exportOpen = true;
      }).then((unlisten) => disposed ? unlisten() : unlisteners.push(unlisten));
    }
    return () => {
      disposed = true;
      waveformRequestSequence += 1;
      window.removeEventListener("keydown", handleKeyboard);
      window.removeEventListener("error", handleWindowError);
      window.removeEventListener("unhandledrejection", handleUnhandledRejection);
      if (playheadSaveTimer !== null) window.clearTimeout(playheadSaveTimer);
      unlisteners.forEach((unlisten) => unlisten());
    };
  });

  async function restoreInitialSession() {
    const demoMode = new URLSearchParams(window.location.search).get("demo");
    const demoRequested = demoMode === "1" || demoMode === "export";
    if (demoRequested || !runningInTauri()) {
      if (demoRequested) {
        activateDemo();
        if (demoMode === "export") activateExportDemo();
      }
      return;
    }
    try {
      const launchSource = await getLaunchSource();
      if (launchSource) {
        const opened = await openSource(launchSource);
        applySession(opened.session, opened.previewUrl);
        return;
      }
      const restored = await getSession();
      if (restored) applySession(restored.session, restored.previewUrl);
    } catch (error) {
      errorMessage = commandMessage(error);
      void recordDiagnosticSafely("frontend_error", `restore_initial_session_failed ${diagnosticText(error)}`);
    }
  }

  async function refreshRecoverableExports() {
    try {
      recoverableExports = await listRecoverableExports();
    } catch (error) {
      errorMessage = `无法检查上次中断的导出：${commandMessage(error)}`;
    }
  }

  async function cleanupRecovery(item: RecoverableExport) {
    recoveryBusyJobId = item.jobId;
    try {
      await cleanupRecoverableExport(item.jobId);
      recoverableExports = recoverableExports.filter((candidate) => candidate.jobId !== item.jobId);
    } catch (error) {
      errorMessage = `无法清理中断文件：${commandMessage(error)}`;
    } finally {
      recoveryBusyJobId = null;
    }
  }

  async function revealRecovery(item: RecoverableExport) {
    try {
      await revealOutput(item.revealPath);
    } catch (error) {
      errorMessage = `无法显示中断文件：${commandMessage(error)}`;
    }
  }

  function activateDemo() {
    demo = true;
    applySession(createDemoSession());
  }

  function activateExportDemo() {
    exportOpen = true;
    exportStarted = {
      jobId: "demo-export",
      encoder: { name: "hevc_videotoolbox", hardwareAccelerated: true, displayName: "Apple VideoToolbox H.265" },
      expectedOutputUs: currentSession?.keptDurationUs ?? 0,
      destination: "/demo/示例课程-公开版.mp4"
    };
    exportProgress = {
      jobId: "demo-export",
      phase: "encoding",
      percent: 63,
      processedSourceUs: 7_290_000_000,
      sourceDurationUs: currentSession?.project.media.durationUs ?? 1,
      speed: "1.84x",
      message: "正在顺序读取源视频并精确重建保留片段"
    };
  }

  function applySession(next: SessionProjection, previewUrl?: string, preservePlayhead = false) {
    const projectChanged = currentSession?.project.projectId !== next.project.projectId;
    const previousPlayheadUs = playheadUs;
    currentSession = next;
    playheadUs = preservePlayhead && !projectChanged ? previousPlayheadUs : next.project.lastPlayheadUs;
    if (projectChanged) {
      resetPlayheadPersistence();
      deletionPlaybackGuard.reset(playheadUs);
      deletionSkipInFlight = false;
      playbackSeekSequence += 1;
      playbackSeekTokens.clear();
      beginWaveformLoad(next.project);
    }
    if (!demo && previewUrl) mediaSourceUrl = previewUrl;
    if (selectedId !== null && !next.project.deleteIntervals.some((item) => item.id === selectedId)) selectedId = null;
  }

  function beginWaveformLoad(nextProject: ProjectV1) {
    const requestSequence = ++waveformRequestSequence;
    const projectId = nextProject.projectId;
    waveform = null;
    if (!nextProject.media.hasAudio) {
      waveformState = "unavailable";
      waveformMessage = "源视频没有音轨";
      return;
    }
    if (demo) {
      waveform = createDemoWaveform(nextProject.media.durationUs);
      waveformState = "ready";
      waveformMessage = "";
      return;
    }

    waveformState = "loading";
    waveformMessage = "正在分析音频…";
    void getAudioWaveform(projectId)
      .then((result) => {
        if (requestSequence !== waveformRequestSequence || currentSession?.project.projectId !== projectId) return;
        waveform = result.peaks.length > 0 ? result : null;
        waveformState = result.peaks.length > 0 ? "ready" : "unavailable";
        waveformMessage = result.peaks.length > 0 ? "" : "音轨中没有可显示的波形";
      })
      .catch((error) => {
        if (requestSequence !== waveformRequestSequence || currentSession?.project.projectId !== projectId) return;
        waveform = null;
        waveformState = "failed";
        waveformMessage = `波形生成失败，可继续剪切：${commandMessage(error)}`;
      });
  }

  function createDemoWaveform(durationUs: number): AudioWaveform {
    const samplesPerSecond = 10;
    const count = Math.max(1, Math.ceil((durationUs / 1_000_000) * samplesPerSecond));
    const peaks = Array.from({ length: count }, (_, index) => {
      const phrasePosition = (index / samplesPerSecond) % 8.5;
      if (phrasePosition > 6.7) return Math.round(2 + 5 * Math.abs(Math.sin(index * 0.19)));
      const envelope = .55 + .45 * Math.sin((phrasePosition / 6.7) * Math.PI);
      return Math.round(28 + 205 * envelope * (.35 + .65 * Math.abs(Math.sin(index * .71))));
    });
    return { samplesPerSecond, peaks };
  }

  async function openRecording() {
    if (editLocked) return;
    if (!runningInTauri()) {
      activateDemo();
      return;
    }
    loading = true;
    errorMessage = null;
    try {
      const path = await chooseSource();
      if (!path) return;
      demo = false;
      const result = await openSource(path);
      applySession(result.session, result.previewUrl);
    } catch (error) {
      errorMessage = commandMessage(error);
      void recordDiagnosticSafely("frontend_error", `open_source_failed ${diagnosticText(error)}`);
    } finally {
      loading = false;
    }
  }

  async function runEdit(
    action: () => Promise<SessionProjection>,
    local?: () => SessionProjection | null
  ): Promise<SessionProjection | null> {
    errorMessage = null;
    if (demo) {
      return local?.() ?? currentSession;
    }
    try {
      const next = await action();
      applySession(next, undefined, true);
      saveWarning = null;
      return next;
    } catch (error) {
      errorMessage = commandMessage(error);
      if (commandCode(error) === "save_failed") saveWarning = errorMessage;
      return null;
    }
  }

  function commandCode(error: unknown): string | null {
    if (!error || typeof error !== "object") return null;
    return (error as Partial<CommandFailure>).code ?? null;
  }

  function resetPlayheadPersistence() {
    if (playheadSaveTimer !== null) window.clearTimeout(playheadSaveTimer);
    playheadSaveTimer = null;
    pendingPlayheadSaveUs = null;
    lastPlayheadSaveAt = Date.now();
  }

  function schedulePlayheadSave(nextUs: number) {
    if (demo || !runningInTauri()) return;
    pendingPlayheadSaveUs = nextUs;
    if (playheadSaveInFlight || playheadSaveTimer !== null) return;
    const delay = Math.max(0, PLAYHEAD_SAVE_INTERVAL_MS - (Date.now() - lastPlayheadSaveAt));
    playheadSaveTimer = window.setTimeout(() => {
      playheadSaveTimer = null;
      void flushPlayheadSave();
    }, delay);
  }

  async function flushPlayheadSave() {
    if (demo || !runningInTauri() || playheadSaveInFlight || pendingPlayheadSaveUs === null) return;
    const value = pendingPlayheadSaveUs;
    const projectId = currentSession?.project.projectId;
    if (!projectId) return;
    pendingPlayheadSaveUs = null;
    playheadSaveInFlight = true;
    try {
      await setPlayhead(value, projectId);
      lastPlayheadSaveAt = Date.now();
      saveWarning = null;
      if (currentSession?.project.projectId === projectId) currentSession.project.lastPlayheadUs = value;
    } catch (error) {
      if (commandCode(error) !== "stale_project") saveWarning = commandMessage(error);
    } finally {
      playheadSaveInFlight = false;
      if (pendingPlayheadSaveUs !== null) schedulePlayheadSave(pendingPlayheadSaveUs);
    }
  }

  function demoUpdate(mutator: (session: SessionProjection) => void): SessionProjection | null {
    if (!currentSession) return null;
    const next = structuredClone(currentSession);
    mutator(next);
    const deleted = next.project.deleteIntervals.reduce((sum, item) => sum + item.endUs - item.startUs, 0);
    next.deletedDurationUs = deleted;
    next.keptDurationUs = next.project.media.durationUs - deleted;
    next.project.reviewedIntervalIds = [];
    next.canUndo = true;
    currentSession = next;
    return next;
  }

  function normalizeDemo(items: DeleteInterval[]): DeleteInterval[] {
    const sorted = [...items].sort((a, b) => a.startUs - b.startUs);
    const result: DeleteInterval[] = [];
    for (const item of sorted) {
      const previous = result.at(-1);
      if (previous && item.startUs <= previous.endUs) {
        previous.endUs = Math.max(previous.endUs, item.endUs);
        previous.id = Math.min(previous.id, item.id);
      } else result.push({ ...item });
    }
    return result;
  }

  async function togglePlayback() {
    try { await player?.togglePlayback(); } catch (error) { errorMessage = commandMessage(error); }
  }

  async function seekTo(nextUs: number, savePosition = true, intent: "manual" | "automatic" = "manual") {
    if (!media) return;
    const targetUs = clampTime(nextUs, media.durationUs);
    const targetProjectId = project?.projectId;
    const seekToken = ++playbackSeekSequence;
    playheadUs = targetUs;
    if (intent === "manual") deletionPlaybackGuard.setManualPosition(targetUs, intervals);
    else deletionPlaybackGuard.setAutomaticPosition(targetUs);
    playbackSeekTokens.add(seekToken);
    let currentSeek = false;
    try {
      await player?.seekTo(targetUs);
      currentSeek = seekToken === playbackSeekSequence && project?.projectId === targetProjectId;
      if (currentSeek) {
        if (intent === "manual") deletionPlaybackGuard.setManualPosition(targetUs, intervals);
        else deletionPlaybackGuard.setAutomaticPosition(targetUs);
      }
    } finally {
      playbackSeekTokens.delete(seekToken);
    }
    if (savePosition && currentSeek) schedulePlayheadSave(targetUs);
    return currentSeek;
  }

  function changeRate(value: number) {
    playbackRate = value;
  }

  function markStart() {
    if (editLocked || markSaving) return;
    pendingStartUs = playheadUs;
  }

  async function markEnd() {
    if (pendingStartUs === null || !project || editLocked || markSaving) return;
    const startUs = pendingStartUs;
    const endUs = playheadUs;
    if (endUs - startUs < frameDurationUs) {
      errorMessage = "删除终点必须至少晚于删除起点一帧。";
      return;
    }
    const projectId = project.projectId;
    markSaving = true;
    try {
      const next = await runEdit(
        () => addDeleteInterval(startUs, endUs, projectId),
        () => demoUpdate((session) => {
          const id = session.project.nextIntervalId++;
          session.project.deleteIntervals = normalizeDemo([...session.project.deleteIntervals, { id, startUs, endUs }]);
        })
      );
      if (!next) return;
      const normalized = findCoveringInterval(next.project.deleteIntervals, startUs, endUs);
      if (!normalized) {
        errorMessage = "删除区间已经保存，但无法定位规范化后的区间；待标记起点已保留，请在区间列表中确认结果。";
        return;
      }
      selectedId = normalized.id;
      pendingStartUs = null;
    } finally {
      markSaving = false;
    }
  }

  function cancelMark() {
    if (editLocked || markSaving) return;
    pendingStartUs = null;
  }

  function findCoveringInterval(items: DeleteInterval[], startUs: number, endUs: number): DeleteInterval | null {
    return items.find((item) => item.startUs <= startUs && item.endUs >= endUs) ?? null;
  }

  async function removeInterval(id: number) {
    if (editLocked) return;
    const projectId = project?.projectId;
    if (!projectId) return;
    await runEdit(
      () => removeDeleteInterval(id, projectId),
      () => demoUpdate((next) => { next.project.deleteIntervals = next.project.deleteIntervals.filter((item) => item.id !== id); })
    );
  }

  async function resizeInterval(event: CustomEvent<{ id: number; startUs: number; endUs: number }>) {
    if (editLocked) return;
    const projectId = project?.projectId;
    if (!projectId) return;
    const { id, startUs, endUs } = event.detail;
    const next = await runEdit(
      () => resizeDeleteInterval(id, startUs, endUs, projectId),
      () => demoUpdate((next) => {
        next.project.deleteIntervals = normalizeDemo(next.project.deleteIntervals.map((item) => item.id === id ? { ...item, startUs, endUs } : item));
      })
    );
    if (!next) return;
    const normalized = findCoveringInterval(next.project.deleteIntervals, startUs, endUs);
    selectedId = normalized?.id ?? null;
    if (!normalized) errorMessage = "区间已经保存，但无法恢复当前选择，请在列表中重新选择。";
  }

  async function toggleReviewed(id: number, reviewed: boolean) {
    if (editLocked) return;
    const projectId = project?.projectId;
    if (!projectId) return;
    await runEdit(
      () => setJoinReviewed(id, reviewed, projectId),
      () => {
        if (!currentSession) return null;
        const next = structuredClone(currentSession);
        next.project.reviewedIntervalIds = reviewed
          ? [...new Set([...next.project.reviewedIntervalIds, id])]
          : next.project.reviewedIntervalIds.filter((item) => item !== id);
        currentSession = next;
        return next;
      }
    );
  }

  async function undo() {
    if (editLocked) return;
    const projectId = project?.projectId;
    if (!demo && projectId) await runEdit(() => undoEdit(projectId));
  }
  async function redo() {
    if (editLocked) return;
    const projectId = project?.projectId;
    if (!demo && projectId) await runEdit(() => redoEdit(projectId));
  }

  function selectInterval(id: number, seek = false) {
    selectedId = id;
    const interval = intervals.find((item) => item.id === id);
    if (seek && interval) void seekTo(interval.startUs);
  }

  function selectIntervalAt(id: number) {
    selectedId = id;
  }

  function beginScrub() {
    if (scrubbing) return;
    scrubbing = true;
    scrubOriginUs = playheadUs;
    resumeAfterScrub = playing;
    player?.pause();
  }

  function previewScrub(nextUs: number) {
    if (!media || !scrubbing) return;
    playheadUs = clampTime(nextUs, media.durationUs);
    player?.previewSeekTo(playheadUs);
  }

  async function commitScrub(nextUs: number) {
    if (!media) return;
    const finalUs = clampTime(nextUs, media.durationUs);
    try {
      playheadUs = finalUs;
      deletionPlaybackGuard.setManualPosition(finalUs, intervals);
      await player?.seekTo(finalUs);
      deletionPlaybackGuard.setManualPosition(finalUs, intervals);
      schedulePlayheadSave(finalUs);
    } catch (error) {
      errorMessage = commandMessage(error);
    } finally {
      scrubbing = false;
      const shouldResume = resumeAfterScrub;
      resumeAfterScrub = false;
      if (shouldResume) {
        try { await player?.play(); }
        catch (error) { errorMessage = commandMessage(error); }
      }
    }
  }

  async function cancelScrub() {
    if (!scrubbing) return;
    const originUs = scrubOriginUs;
    try {
      playheadUs = originUs;
      deletionPlaybackGuard.setManualPosition(originUs, intervals);
      await player?.seekTo(originUs);
      deletionPlaybackGuard.setManualPosition(originUs, intervals);
    } catch (error) {
      errorMessage = commandMessage(error);
    } finally {
      scrubbing = false;
      const shouldResume = resumeAfterScrub;
      resumeAfterScrub = false;
      if (shouldResume) {
        try { await player?.play(); }
        catch (error) { errorMessage = commandMessage(error); }
      }
    }
  }

  async function previewJoin(index: number) {
    const interval = intervals[index];
    if (!interval) return;
    reviewIndex = index;
    selectedId = interval.id;
    reviewOpen = false;
    reviewJump = {
      startUs: interval.startUs,
      endUs: interval.endUs,
      stopUs: Math.min(project?.media.durationUs ?? interval.endUs + 3_000_000, interval.endUs + 3_000_000),
      jumped: false
    };
    await seekTo(Math.max(0, interval.startUs - 3_000_000), false);
    await player?.play();
  }

  function handleTime(nextUs: number) {
    if (scrubbing || playbackSeekTokens.size > 0) return;
    playheadUs = nextUs;
    schedulePlayheadSave(nextUs);
    if (reviewJump) {
      deletionPlaybackGuard.setAutomaticPosition(nextUs);
      if (!reviewJump.jumped && nextUs >= reviewJump.startUs) {
        reviewJump.jumped = true;
        void seekTo(reviewJump.endUs, false, "automatic").then((currentSeek) => currentSeek ? player?.play() : undefined);
      } else if (reviewJump.jumped && nextUs >= reviewJump.stopUs) {
        player?.pause();
        reviewJump = null;
        reviewOpen = true;
      }
      return;
    }

    if (deletionSkipInFlight) return;
    const interval = deletionPlaybackGuard.observePlayback(nextUs, playing, intervals);
    if (!interval) return;

    deletionSkipInFlight = true;
    const targetUs = Math.max(nextUs, interval.endUs);
    void seekTo(targetUs, false, "automatic")
      .then((currentSeek) => currentSeek ? player?.play() : undefined)
      .catch((error) => {
        player?.pause();
        errorMessage = `无法自动跳过删除区间：${commandMessage(error)}`;
      })
      .finally(() => deletionSkipInFlight = false);
  }

  async function beginExport(allowUnreviewed: boolean, allowBitDepthFallback: boolean) {
    if (!project) return;
    const stem = sourceName.replace(/\.mp4$/i, "");
    const destination = runningInTauri() ? await chooseDestination(`${stem}-公开版.mp4`) : null;
    if (!destination && runningInTauri()) return;
    if (!destination) return;
    reviewOpen = false;
    exportOpen = true;
    exportPreparing = true;
    exportStarted = null;
    exportProgress = null;
    exportResult = null;
    errorMessage = null;
    try {
      exportStarted = await startExport(destination, allowUnreviewed, allowBitDepthFallback);
      exportPreparing = false;
    } catch (error) {
      exportOpen = false;
      exportPreparing = false;
      errorMessage = commandMessage(error);
    }
  }

  async function cancelCurrentExport() {
    if (!exportStarted) return;
    try { await cancelExport(exportStarted.jobId); }
    catch (error) { errorMessage = commandMessage(error); }
  }

  async function revealExport(path: string) {
    try { await revealOutput(path); }
    catch (error) { errorMessage = commandMessage(error); }
  }

  async function revealDiagnostics() {
    try {
      await revealDiagnosticLog();
    } catch (error) {
      errorMessage = `无法打开诊断日志：${commandMessage(error)}`;
    }
  }

  async function handlePlayerFailure(message: string) {
    void recordDiagnosticSafely("player_error", message);
    if (!media || demo || !runningInTauri()) {
      errorMessage = message;
      return;
    }
    errorMessage = "系统预览组件无法播放，正在用 FFmpeg 检查源视频…";
    try {
      const diagnostic = await diagnosePlayback();
      if (diagnostic.ffmpegCanDecode) {
        const codec = media.videoCodec === "hevc" ? "H.265 / HEVC" : "H.264 / AVC";
        const windowsHint = navigator.userAgent.includes("Windows") && media.videoCodec === "hevc"
          ? "请安装或修复 Microsoft HEVC Video Extensions。"
          : "可以继续使用导出功能，或改用系统支持该编码的环境预览。";
        errorMessage = `源视频可被 FFmpeg 正常解码，但本地预览流未能由系统组件播放 ${codec}。${windowsHint}`;
      } else {
        errorMessage = "系统预览组件和 FFmpeg 都无法解码这段视频；文件可能损坏或使用了不受支持的编码参数。";
      }
    } catch (error) {
      errorMessage = `${message}；FFmpeg 诊断也未能完成：${commandMessage(error)}`;
    }
  }

  function handleKeyboard(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null;
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "o") {
      event.preventDefault();
      if (!editLocked) void openRecording();
      return;
    }
    if (target?.matches("button, input, textarea, select, [contenteditable=true], [role=slider], [role=scrollbar]")) return;
    if (!currentSession) return;
    if (event.key === "Escape") { if (!editLocked) pendingStartUs = null; reviewOpen = false; return; }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "z") {
      event.preventDefault();
      if (!editLocked) void (event.shiftKey ? redo() : undo());
      return;
    }
    const lower = event.key.toLowerCase();
    if (event.code === "Space") { event.preventDefault(); void togglePlayback(); }
    else if (event.key === "ArrowLeft") { event.preventDefault(); void seekTo(playheadUs - (event.metaKey || event.ctrlKey ? 30_000_000 : event.shiftKey ? 5_000_000 : 1_000_000)); }
    else if (event.key === "ArrowRight") { event.preventDefault(); void seekTo(playheadUs + (event.metaKey || event.ctrlKey ? 30_000_000 : event.shiftKey ? 5_000_000 : 1_000_000)); }
    else if (!editLocked && (lower === "i" || event.key === "[")) markStart();
    else if (!editLocked && (lower === "o" || event.key === "]")) void markEnd();
    else if (lower === "j") changeRate(Math.max(.5, playbackRate - .25));
    else if (lower === "k") player?.pause();
    else if (lower === "l") changeRate(Math.min(2, playbackRate + .25));
    else if (!editLocked && (event.key === "Delete" || event.key === "Backspace") && selectedId !== null) void removeInterval(selectedId);
  }

  function fileName(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

  function diagnosticText(value: unknown): string {
    if (value instanceof Error) return `${value.name}: ${value.message}${value.stack ? ` ${value.stack}` : ""}`;
    if (typeof value === "string") return value;
    try { return JSON.stringify(value); }
    catch { return String(value); }
  }

  async function recordDiagnosticSafely(
    kind: "frontend_ready" | "frontend_error" | "unhandled_rejection" | "player_error",
    message: string
  ) {
    if (!runningInTauri()) return;
    try { await recordFrontendDiagnostic(kind, message); }
    catch { /* 诊断功能本身不能影响剪辑流程。 */ }
  }
</script>

<svelte:head><title>{project ? `${sourceName} · SpyCut` : "SpyCut · 精确删除区间"}</title></svelte:head>

{#if !currentSession}
  <EmptyState {loading} on:open={openRecording} on:demo={activateDemo} />
  {#if runningInTauri()}<button class="empty-diagnostics" type="button" on:click={revealDiagnostics}>打开诊断日志</button>{/if}
{:else if project && media}
  <main class="workspace">
    <header class="app-header">
      <div class="app-brand"><div class="brand-slash small" aria-hidden="true"><i></i><i></i></div><strong>SpyCut</strong><span>V1</span></div>
      <div class="file-identity">
        <p>{sourceName}</p>
        <div><span>{media.videoCodec === "hevc" ? "H.265 / HEVC" : "H.264 / AVC"}</span><span>{media.width}×{media.height}</span><span>{(media.frameRate.num / media.frameRate.den).toFixed(2)} FPS</span><span>{formatBytes(project.source.sizeBytes)}</span></div>
      </div>
      <div class="header-actions">
        {#if demo}<span class="demo-badge">界面演示</span>{/if}
        {#if !demo}<button type="button" on:click={revealDiagnostics}>诊断日志</button>{/if}
        <button type="button" disabled={editLocked} on:click={openRecording}>更换录屏</button>
        <button class="primary compact" type="button" disabled={intervals.length === 0 || exportOpen} on:click={() => reviewOpen = true}>检查并导出 <span>→</span></button>
      </div>
    </header>

    <section class="work-grid">
      <div class="video-column">
        <PlayerPane
          bind:this={player}
          sourceUrl={mediaSourceUrl}
          {demo}
          {playbackRate}
          on:time={(event: CustomEvent<{ playheadUs: number }>) => handleTime(event.detail.playheadUs)}
          on:state={(event: CustomEvent<{ playing: boolean }>) => playing = event.detail.playing}
          on:error={(event: CustomEvent<{ message: string }>) => handlePlayerFailure(event.detail.message)}
        />
        <TransportControls
          {playheadUs}
          durationUs={media.durationUs}
          {playing}
          {playbackRate}
          on:play={togglePlayback}
          on:seek={(event) => seekTo(playheadUs + event.detail.deltaUs)}
          on:rate={(event) => changeRate(event.detail.value)}
        />
      </div>
      <IntervalList
        {intervals}
        {reviewedIds}
        {selectedId}
        locked={editLocked}
        on:select={(event) => selectInterval(event.detail.id, true)}
        on:remove={(event) => removeInterval(event.detail.id)}
        on:reviewed={(event) => toggleReviewed(event.detail.id, event.detail.reviewed)}
      />
    </section>

    <TimelineEditor
      durationUs={media.durationUs}
      {frameDurationUs}
      {playheadUs}
      {intervals}
      {selectedId}
      {pendingStartUs}
      canUndo={currentSession.canUndo}
      canRedo={currentSession.canRedo}
      {playing}
      {editLocked}
      {markSaving}
      {waveform}
      {waveformState}
      {waveformMessage}
      on:scrubStart={beginScrub}
      on:scrubPreview={(event) => previewScrub(event.detail.playheadUs)}
      on:scrubCommit={(event) => commitScrub(event.detail.playheadUs)}
      on:scrubCancel={cancelScrub}
      on:select={(event) => selectIntervalAt(event.detail.id)}
      on:resize={resizeInterval}
      on:markStart={markStart}
      on:markEnd={markEnd}
      on:cancelMark={cancelMark}
      on:undo={undo}
      on:redo={redo}
    />

    <footer class="status-bar">
      <div class:save-error={saveWarning !== null}><span class="status-light"></span><strong>{saveWarning ? "项目保存失败" : "项目已自动保存"}</strong><i>{saveWarning ?? "原视频只读 · 设置保存在同目录 JSON"}</i></div>
      <div class="selected-status">
        {#if selectedInterval}<span>已选 DELETE {String(intervals.indexOf(selectedInterval) + 1).padStart(2, "0")}</span><strong>{formatTime(selectedInterval.startUs, true)} — {formatTime(selectedInterval.endUs, true)}</strong>{:else}<span>点击红色区间进行复核或调整边界</span>{/if}
      </div>
      <div><span>已删除</span><strong class="danger-text">{formatDurationCompact(currentSession.deletedDurationUs)}</strong><span>预计保留</span><strong>{formatDurationCompact(currentSession.keptDurationUs)}</strong></div>
    </footer>
  </main>

  {#if reviewOpen}
    <ReviewPanel
      session={currentSession}
      activeIndex={reviewIndex}
      on:close={() => reviewOpen = false}
      on:navigate={(event) => reviewIndex = event.detail.index}
      on:preview={(event) => previewJoin(event.detail.index)}
      on:reviewed={(event) => toggleReviewed(event.detail.id, event.detail.reviewed)}
      on:export={(event) => beginExport(event.detail.allowUnreviewed, event.detail.allowBitDepthFallback)}
    />
  {/if}

  {#if exportOpen}
    <ExportPanel
      preparing={exportPreparing}
      started={exportStarted}
      progress={exportProgress}
      result={exportResult}
      on:cancel={cancelCurrentExport}
      on:close={() => exportOpen = false}
      on:reveal={(event) => revealExport(event.detail.path)}
    />
  {/if}
{/if}

{#if errorMessage}<ErrorBanner message={errorMessage} on:dismiss={() => errorMessage = null} />{/if}

{#if recoverableExports.length > 0}
  <aside class="recovery-notice" aria-live="polite">
    <header><strong>发现 {recoverableExports.length} 个上次中断的导出</strong><span>最终文件没有被覆盖，可以查看或清理 SpyCut 自己生成的临时文件。</span></header>
    {#each recoverableExports as item}
      <div>
        <p title={item.destinationPath}>{fileName(item.destinationPath)}{#if item.partialSizeBytes > 0}<small>{formatBytes(item.partialSizeBytes)}</small>{/if}</p>
        <button type="button" class="secondary" on:click={() => revealRecovery(item)}>在文件夹中显示</button>
        <button type="button" disabled={recoveryBusyJobId === item.jobId} on:click={() => cleanupRecovery(item)}>{recoveryBusyJobId === item.jobId ? "清理中…" : "清理临时文件"}</button>
      </div>
    {/each}
  </aside>
{/if}
