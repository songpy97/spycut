import type { MediaPlayerAdapter } from "./MediaPlayerAdapter";

const SEEK_TIMEOUT_MS = 10_000;
const SEEK_TOLERANCE_SECONDS = 0.0005;

export class HtmlVideoAdapter implements MediaPlayerAdapter {
  private playbackSequence = 0;
  private pendingSeekSupersede: (() => void) | null = null;

  constructor(private readonly video: HTMLVideoElement) {}

  async load(sourceUrl: string): Promise<void> {
    this.invalidatePlayback();
    this.supersedePendingSeek();
    this.video.src = sourceUrl;
    this.video.load();
    await new Promise<void>((resolve, reject) => {
      const timer = window.setTimeout(() => reject(new Error("视频加载超时")), 15_000);
      const ready = () => {
        window.clearTimeout(timer);
        cleanup();
        resolve();
      };
      const failed = () => {
        window.clearTimeout(timer);
        cleanup();
        reject(new Error("当前系统视频组件无法播放这个文件"));
      };
      const cleanup = () => {
        this.video.removeEventListener("loadedmetadata", ready);
        this.video.removeEventListener("error", failed);
      };
      this.video.addEventListener("loadedmetadata", ready, { once: true });
      this.video.addEventListener("error", failed, { once: true });
    });
  }

  async play(): Promise<void> {
    const playbackSequence = ++this.playbackSequence;
    try {
      await this.video.play();
    } catch (error) {
      if (playbackSequence !== this.playbackSequence && isExpectedPlayInterruption(error)) return;
      throw error;
    }
  }

  pause(): void {
    this.invalidatePlayback();
    this.video.pause();
  }

  previewSeekTo(seconds: number): void {
    if (!Number.isFinite(seconds)) return;
    this.supersedePendingSeek();
    this.video.currentTime = this.clampTarget(seconds);
  }

  async seekTo(seconds: number): Promise<boolean> {
    if (!Number.isFinite(seconds)) throw new Error("无效的预览时间");
    const target = this.clampTarget(seconds);
    this.supersedePendingSeek();
    if (!this.video.seeking && this.isAtTarget(target)) return true;

    return new Promise<boolean>((resolve, reject) => {
      let settled = false;
      let timer = 0;
      const supersede = () => finish(false);
      const seeked = () => {
        if (!this.video.seeking && this.isAtTarget(target)) finish(true);
      };
      const failed = () => finish(false, new Error("视频定位失败"));
      const cleanup = () => {
        window.clearTimeout(timer);
        this.video.removeEventListener("seeked", seeked);
        this.video.removeEventListener("error", failed);
        if (this.pendingSeekSupersede === supersede) this.pendingSeekSupersede = null;
      };
      const finish = (completed: boolean, error?: Error) => {
        if (settled) return;
        settled = true;
        cleanup();
        if (error) reject(error); else resolve(completed);
      };

      this.video.addEventListener("seeked", seeked);
      this.video.addEventListener("error", failed, { once: true });
      this.pendingSeekSupersede = supersede;
      timer = window.setTimeout(() => {
        if (!this.video.seeking && this.isAtTarget(target)) finish(true);
        else finish(false, new Error("视频定位超时"));
      }, SEEK_TIMEOUT_MS);
      try {
        this.video.currentTime = target;
      } catch (error) {
        finish(false, error instanceof Error ? error : new Error("视频定位失败"));
        return;
      }
      queueMicrotask(() => {
        if (!this.video.seeking && this.isAtTarget(target)) finish(true);
      });
    });
  }

  setRate(rate: number): void {
    this.video.playbackRate = rate;
  }

  currentTimeSeconds(): number {
    return this.video.currentTime;
  }

  dispose(): void {
    this.supersedePendingSeek();
    this.invalidatePlayback();
    this.video.pause();
    this.video.removeAttribute("src");
    this.video.load();
  }

  private clampTarget(seconds: number): number {
    const target = Math.max(0, seconds);
    const duration = this.video.duration;
    return Number.isFinite(duration) && duration >= 0 ? Math.min(target, duration) : target;
  }

  private isAtTarget(target: number): boolean {
    return Math.abs(this.video.currentTime - target) <= SEEK_TOLERANCE_SECONDS;
  }

  private invalidatePlayback(): void {
    this.playbackSequence += 1;
  }

  private supersedePendingSeek(): void {
    const supersede = this.pendingSeekSupersede;
    this.pendingSeekSupersede = null;
    supersede?.();
  }
}

function isExpectedPlayInterruption(error: unknown): boolean {
  if (!error || typeof error !== "object") return false;
  const candidate = error as { name?: unknown; message?: unknown };
  if (candidate.name === "AbortError") return true;
  return typeof candidate.message === "string"
    && /play\(\).*interrupted.*pause\(\)/i.test(candidate.message);
}
