import type { MediaPlayerAdapter } from "./MediaPlayerAdapter";

export class HtmlVideoAdapter implements MediaPlayerAdapter {
  constructor(private readonly video: HTMLVideoElement) {}

  async load(sourceUrl: string): Promise<void> {
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

  play(): Promise<void> {
    return this.video.play();
  }

  pause(): void {
    this.video.pause();
  }

  previewSeekTo(seconds: number): void {
    if (!Number.isFinite(seconds)) return;
    this.video.currentTime = Math.max(0, seconds);
  }

  async seekTo(seconds: number): Promise<void> {
    const target = Math.max(0, seconds);
    if (!Number.isFinite(target)) throw new Error("无效的预览时间");
    if (Math.abs(this.video.currentTime - target) <= 0.0005) return;

    await new Promise<void>((resolve, reject) => {
      const timer = window.setTimeout(() => finish(new Error("视频定位超时")), 5_000);
      const seeked = () => finish();
      const failed = () => finish(new Error("视频定位失败"));
      const cleanup = () => {
        window.clearTimeout(timer);
        this.video.removeEventListener("seeked", seeked);
        this.video.removeEventListener("error", failed);
      };
      const finish = (error?: Error) => {
        cleanup();
        if (error) reject(error); else resolve();
      };

      this.video.addEventListener("seeked", seeked, { once: true });
      this.video.addEventListener("error", failed, { once: true });
      try {
        this.video.currentTime = target;
      } catch (error) {
        finish(error instanceof Error ? error : new Error("视频定位失败"));
      }
    });
  }

  setRate(rate: number): void {
    this.video.playbackRate = rate;
  }

  currentTimeSeconds(): number {
    return this.video.currentTime;
  }

  dispose(): void {
    this.video.pause();
    this.video.removeAttribute("src");
    this.video.load();
  }
}
