// Pure helpers for the OCR model picker, kept out of the dialog so they are
// directly testable.
import type { DownloadProgress, OcrModel, OcrModelStatus } from "../ipc/types";

export interface ModelOption {
  id: OcrModel;
  label: string;
  blurb: string;
}

export function modelOptions(status: OcrModelStatus | null): ModelOption[] {
  const size = status ? megabytes(status.accurate_download_bytes) : "约 20 MB";
  return [
    {
      id: "fast",
      label: "快速（默认）",
      blurb: "PP-OCRv6 tiny，内置，无需下载。一张截图约 0.3 秒。",
    },
    {
      id: "accurate",
      label: "高精度",
      blurb: status?.accurate_installed
        ? "PP-OCRv6 small，已下载。约 0.7 秒，大小写与标点更准确。"
        : `PP-OCRv6 small，需一次性下载 ${size}。约 0.7 秒，大小写与标点更准确。`,
    },
  ];
}

/** Whether an image can be imported with `model` right now. */
export function modelReady(model: OcrModel, status: OcrModelStatus | null): boolean {
  return model === "fast" || status?.accurate_installed === true;
}

export function megabytes(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

export function progressLabel(progress: DownloadProgress): string {
  const percent = progress.total > 0 ? Math.floor((progress.done * 100) / progress.total) : 0;
  return `已下载 ${megabytes(progress.done)} / ${megabytes(progress.total)}（${String(percent)}%）`;
}
