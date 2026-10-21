import { describe, expect, it } from "vitest";
import { tauriArgs } from "./tauriArgs.mjs";

const extra = (args) => JSON.parse(args[args.lastIndexOf("--config") + 1]);

describe("tauri build arguments", () => {
  it("leaves other commands alone", () => {
    expect(tauriArgs(["dev"], { TAURI_SIGNING_PRIVATE_KEY: "k" }).args).toEqual(["dev"]);
    expect(tauriArgs(["signer", "generate"], {}).args).toEqual(["signer", "generate"]);
  });

  it("builds updater artifacts only with the signing key", () => {
    const release = ["build", "--config", "src-tauri/tauri.agents.conf.json"];
    const signed = tauriArgs(release, { TAURI_SIGNING_PRIVATE_KEY: "secret", FORGE_UPDATER_PUBKEY: "pub" });
    expect(signed.args.slice(0, 3)).toEqual(release);
    expect(extra(signed.args)).toEqual({
      bundle: { createUpdaterArtifacts: true },
      plugins: { updater: { pubkey: "pub" } },
    });
    expect(signed.notes).toEqual([]);

    const local = tauriArgs(["build", "--bundles", "appimage"], {});
    expect(extra(local.args)).toEqual({ bundle: { createUpdaterArtifacts: false } });
    expect(local.notes.join(" ")).toContain("will not check for updates");
  });

  it("keeps cargo arguments after --", () => {
    const { args } = tauriArgs(["build", "--debug", "--", "--locked"], { TAURI_SIGNING_PRIVATE_KEY: " " });
    expect(args.slice(-2)).toEqual(["--", "--locked"]);
    expect(args[2]).toBe("--config");
  });
});
