import { describe, expect, it } from "vitest";
import type { CommandLine } from "@/ipc/run";
import type { DeviceSummary, EmbeddedInfo, RegisterValue } from "@/ipc/cppTools";

// The PlatformIO demo is chosen by the page URL, before the mocks load.
history.replaceState(null, "", "/?project=pio");
const { invoke } = await import("@/ipc");
await import("./settingsMock");
await import("./testFrameworksMock");
await import("./workspaceMock");
await import("./runMock");
await import("./dapMock");
await import("./cppMock");

describe("CForge embedded mock (Rust crates through WebAssembly)", () => {
  it("reads platformio.ini and builds PlatformIO task commands", async () => {
    const info = await invoke<EmbeddedInfo>("embedded_info");
    expect(info.kinds.platformio).toBe(true);
    expect(info.platformio?.envs.map((e) => [e.name, e.board, e.default])).toEqual([
      ["nucleo_f401re", "nucleo_f401re", true],
      ["esp32dev", "esp32dev", false],
      ["native", "", false],
    ]);
    expect(info.platformio?.envs[0].options.build_flags).toBe("-Wall -DBLINK_MS=250 -DLED_PIN=PA5");
    const upload = await invoke<CommandLine>("pio_command", { env: "esp32dev", task: "upload" });
    expect(upload.args).toEqual(["run", "-e", "esp32dev", "-t", "upload"]);
  });

  it("decodes the SVD file's registers for the Peripherals view", async () => {
    const device = await invoke<DeviceSummary>("cpp_svd_peripherals", { path: "STM32F401.svd" });
    expect(device.peripherals.map((p) => p.name)).toEqual(["GPIOA", "GPIOB", "RCC", "USART2", "TIM2"]);
    const regs = await invoke<RegisterValue[]>("cpp_svd_read", { path: "STM32F401.svd", peripheral: "GPIOA", sessionId: "dap-embedded-1" });
    const moder = regs.find((r) => r.name === "MODER")!;
    expect(moder.display).toBe("0xA8000400");
    expect(moder.fields.find((f) => f.name === "MODER5")?.display).toBe("Output (1)");
    expect(regs.find((r) => r.name === "BSRR")?.display).toBe("write-only");
  });
});
