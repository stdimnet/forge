import { describe, expect, it } from "vitest";
import type { DownloadProgress } from "@/ipc/updates";
import { downloadFraction, downloadLabel, formatSpeed, formatTransferred, isFinished, signatureLabel } from "./format";

const MB = 1024 * 1024;
const p = (over: Partial<DownloadProgress>): DownloadProgress => ({
  version: "0.2.0",
  phase: "downloading",
  downloaded: 0,
  total: 94 * MB,
  bytesPerSecond: 0,
  ...over,
});

describe("update download text", () => {
  it("shows megabytes and speed", () => {
    expect(formatTransferred(12 * MB, 94 * MB)).toBe("12 / 94 MB");
    expect(formatTransferred(3 * MB, null)).toBe("3 MB");
    expect(formatSpeed(9.8 * MB)).toBe("9.8 MB/s");
    expect(formatSpeed(0)).toBe("");
  });

  it("follows the phases", () => {
    expect(downloadFraction(null)).toBe(0);
    expect(downloadFraction(p({ downloaded: 47 * MB }))).toBeCloseTo(0.5);
    expect(downloadFraction(p({ phase: "verifying" }))).toBe(1);
    expect(downloadLabel("0.2.0", null)).toBe("Downloading 0.2.0");
    expect(downloadLabel("0.2.0", p({ phase: "installing" }))).toBe("Installing 0.2.0");
    expect(signatureLabel(p({ phase: "verifying" })).text).toBe("Signature: verifying…");
    expect(signatureLabel(p({ phase: "verified" }))).toEqual({ text: "Signature: valid", tone: "ok" });
    expect(signatureLabel(p({ phase: "failed", message: "bad signature" }))).toEqual({ text: "bad signature", tone: "error" });
    expect(isFinished("simulated")).toBe(true);
    expect(isFinished("verified")).toBe(false);
  });
});
