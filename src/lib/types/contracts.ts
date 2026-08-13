export type VideoCodec = "h264" | "hevc";

export interface FrameRate {
  num: number;
  den: number;
}

export interface MediaInfo {
  durationUs: number;
  container: string;
  videoCodec: VideoCodec;
  width: number;
  height: number;
  frameRate: FrameRate;
  variableFrameRate: boolean;
  videoStreamCount: number;
  audioStreamCount: number;
  pixelFormat: string | null;
  bitDepth: number | null;
  videoBitRate: number | null;
  hasAudio: boolean;
  audioCodec: string | null;
  audioSampleRate: number | null;
  audioChannels: number | null;
  audioBitRate: number | null;
}

export interface SourceIdentity {
  canonicalPath: string;
  sizeBytes: number;
  modifiedUnixMs: number;
  edgeHashBlake3: string;
}

export interface DeleteInterval {
  id: number;
  startUs: number;
  endUs: number;
}

export interface ProjectV1 {
  schemaVersion: number;
  projectId: string;
  source: SourceIdentity;
  media: MediaInfo;
  deleteIntervals: DeleteInterval[];
  nextIntervalId: number;
  lastPlayheadUs: number;
  reviewedIntervalIds: number[];
  createdUnixMs: number;
  updatedUnixMs: number;
}

export interface SessionProjection {
  project: ProjectV1;
  canUndo: boolean;
  canRedo: boolean;
  deletedDurationUs: number;
  keptDurationUs: number;
}

export interface OpenSourceResult {
  session: SessionProjection;
  resumed: boolean;
  previewUrl: string;
}

export interface CommandFailure {
  code: string;
  message: string;
}

export interface DiagnosticStatus {
  available: boolean;
  logPath: string;
  previousSessionUnclean: boolean;
}

export type ExportPhase = "preparing" | "encoding" | "validating" | "finalizing";
export type ExportOutcomeStatus = "completed" | "cancelled" | "failed";

export interface EncoderSelection {
  name: string;
  hardwareAccelerated: boolean;
  displayName: string;
}

export interface ExportStarted {
  jobId: string;
  encoder: EncoderSelection;
  expectedOutputUs: number;
  destination: string;
}

export interface ExportProgress {
  jobId: string;
  phase: ExportPhase;
  percent: number;
  processedSourceUs: number;
  sourceDurationUs: number;
  speed: string | null;
  message: string;
}

export interface ValidationSummary {
  actualDurationUs: number;
  expectedDurationUs: number;
  durationDeltaUs: number;
  avDurationDeltaUs: number | null;
  startTimeUs: number;
  outputSizeBytes: number;
  decodedCheckpoints: number;
}

export interface ExportResult {
  jobId: string;
  status: ExportOutcomeStatus;
  outputPath: string | null;
  message: string;
  validation: ValidationSummary | null;
}

export interface ActiveExportProjection {
  jobId: string;
}

export interface RecoverableExport {
  jobId: string;
  destinationPath: string;
  partialPath: string | null;
  revealPath: string;
  partialSizeBytes: number;
  createdUnixMs: number;
}

export interface PlaybackDiagnostic {
  ffmpegCanDecode: boolean;
  details: string;
}

export interface AudioWaveform {
  samplesPerSecond: number;
  peaks: number[];
}

export function createDemoSession(): SessionProjection {
  const hour = 3_600_000_000;
  const intervals: DeleteInterval[] = [
    { id: 1, startUs: 12 * 60_000_000 + 14_000_000, endUs: 13 * 60_000_000 + 2_000_000 },
    { id: 2, startUs: 37 * 60_000_000 + 18_000_000, endUs: 41 * 60_000_000 + 30_000_000 },
    { id: 3, startUs: hour + 6 * 60_000_000 + 10_000_000, endUs: hour + 8 * 60_000_000 + 44_000_000 },
    { id: 4, startUs: 2 * hour + 12 * 60_000_000, endUs: 2 * hour + 28 * 60_000_000 }
  ];
  const durationUs = 3 * hour + 12 * 60_000_000 + 25_000_000;
  const deletedDurationUs = intervals.reduce((sum, item) => sum + item.endUs - item.startUs, 0);

  return {
    project: {
      schemaVersion: 1,
      projectId: "demo",
      source: {
        canonicalPath: "/demo/示例课程.mp4",
        sizeBytes: 18_420_000_000,
        modifiedUnixMs: Date.now(),
        edgeHashBlake3: "demo"
      },
      media: {
        durationUs,
        container: "mp4",
        videoCodec: "hevc",
        width: 3840,
        height: 2160,
        frameRate: { num: 30, den: 1 },
        variableFrameRate: false,
        videoStreamCount: 1,
        audioStreamCount: 1,
        pixelFormat: "yuv420p",
        bitDepth: 8,
        videoBitRate: 12_000_000,
        hasAudio: true,
        audioCodec: "aac",
        audioSampleRate: 48_000,
        audioChannels: 2,
        audioBitRate: 160_000
      },
      deleteIntervals: intervals,
      nextIntervalId: 5,
      lastPlayheadUs: hour + 7 * 60_000_000 + 11_000_000,
      reviewedIntervalIds: [1, 2],
      createdUnixMs: Date.now(),
      updatedUnixMs: Date.now()
    },
    canUndo: true,
    canRedo: false,
    deletedDurationUs,
    keptDurationUs: durationUs - deletedDurationUs
  };
}
