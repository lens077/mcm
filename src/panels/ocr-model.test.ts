import { describe, expect, it } from "vitest";
import { megabytes, modelOptions, modelReady, progressLabel } from "./ocr-model";
import type { OcrModelStatus } from "../ipc/types";

const status = (installed: boolean): OcrModelStatus => ({
  accurate_installed: installed,
  accurate_download_bytes: 21_234_325,
  accurate_dir: "/data/models/pp-ocrv6-small",
  downloading: false,
});

describe("OCR model picker", () => {
  it("defaults to the built-in fast model, always ready", () => {
    expect(modelOptions(null)[0]?.id).toBe("fast");
    expect(modelReady("fast", null)).toBe(true);
    expect(modelReady("fast", status(false))).toBe(true);
  });

  it("needs the accurate model downloaded before use", () => {
    expect(modelReady("accurate", null)).toBe(false);
    expect(modelReady("accurate", status(false))).toBe(false);
    expect(modelReady("accurate", status(true))).toBe(true);
  });

  it("states the download size until installed", () => {
    expect(modelOptions(status(false))[1]?.blurb).toContain("21.2 MB");
    expect(modelOptions(status(true))[1]?.blurb).toContain("已下载");
  });

  it("formats progress", () => {
    expect(megabytes(21_234_325)).toBe("21.2 MB");
    expect(progressLabel({ done: 10_617_163, total: 21_234_325 })).toBe(
      "已下载 10.6 MB / 21.2 MB（50%）",
    );
    expect(progressLabel({ done: 0, total: 0 })).toContain("0%");
  });
});
