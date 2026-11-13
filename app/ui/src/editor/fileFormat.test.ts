import { describe, expect, it, vi } from "vitest";
import type { FileFormat } from "@/ipc/text";

const m = vi.hoisted(() => ({ replies: [] as ((f: unknown) => void)[], setTabFormat: vi.fn() }));
vi.mock("@/ipc/text", async (orig) => {
  const real = await orig<typeof import("@/ipc/text")>();
  return { ...real, textApi: { ...real.textApi, fileFormat: () => new Promise((r) => m.replies.push(r)) } };
});
vi.mock("@/state/editor", async (orig) => ({
  ...(await orig<typeof import("@/state/editor")>()),
  editor: { tabs: [{ path: "a.php", content: "<?php\n" }] },
  fileSystemFor: () => undefined,
  setTabFormat: m.setTabFormat,
}));

const { refreshFormat } = await import("./fileFormat");
const format = (lineSeparator: string) => ({ lineSeparator }) as unknown as FileFormat;

describe("refreshFormat", () => {
  it("applies the newest answer for a file, also when an older one arrives last", async () => {
    // Resolve what the module asked for when it loaded first.
    m.replies.splice(0).forEach((r) => r(format("\n")));
    await new Promise((r) => setTimeout(r, 0));
    m.setTabFormat.mockClear();
    const older = refreshFormat("a.php");
    const newer = refreshFormat("a.php");
    m.replies[1](format("\r\n"));
    await newer;
    m.replies[0](format("\n"));
    await older;
    expect(m.setTabFormat).toHaveBeenCalledTimes(1);
    expect(m.setTabFormat).toHaveBeenLastCalledWith("a.php", format("\r\n"));
  });
});
