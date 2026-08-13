export interface MediaPlayerAdapter {
  load(sourceUrl: string): Promise<void>;
  play(): Promise<void>;
  pause(): void;
  previewSeekTo(seconds: number): void;
  seekTo(seconds: number): Promise<void>;
  setRate(rate: number): void;
  currentTimeSeconds(): number;
  dispose(): void;
}
