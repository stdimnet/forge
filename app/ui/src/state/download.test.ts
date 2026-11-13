import { afterEach, describe, expect, it, vi } from "vitest";
import { DOWNLOAD_URL_LIFETIME_MS, downloadBlob } from "./download";

afterEach(() => vi.useRealTimers());

describe("downloadBlob", () => {
  it("revokes the object URL only after the browser had time to start the download", () => {
    vi.useFakeTimers();
    const create = vi.fn(() => "blob:x");
    const revoke = vi.fn();
    Object.assign(URL, { createObjectURL: create, revokeObjectURL: revoke });
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
    downloadBlob("report.html", new Blob(["<p>"]));
    expect(click).toHaveBeenCalledOnce();
    expect(revoke).not.toHaveBeenCalled();
    vi.advanceTimersByTime(DOWNLOAD_URL_LIFETIME_MS);
    expect(revoke).toHaveBeenCalledWith("blob:x");
  });
});
