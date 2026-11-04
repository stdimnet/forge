import { describe, expect, it } from "vitest";
import { DEFAULT_TIMESTAMP_URL, tauriArgs, windowsSigning } from "./tauriArgs.mjs";

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

  it("signs Windows bundles only on Windows and only with signing material", () => {
    const thumbprint = "ab12".repeat(10);
    const env = { FORGE_WINDOWS_CERT_THUMBPRINT: thumbprint };
    expect(extra(tauriArgs(["build"], env, "win32").args).bundle.windows).toEqual({
      certificateThumbprint: thumbprint.toUpperCase(),
      digestAlgorithm: "sha256",
      timestampUrl: DEFAULT_TIMESTAMP_URL,
    });
    expect(extra(tauriArgs(["build"], env, "linux").args).bundle.windows).toBeUndefined();
    const unsigned = tauriArgs(["build"], {}, "win32");
    expect(extra(unsigned.args).bundle.windows).toBeUndefined();
    expect(unsigned.notes.join(" ")).toContain("not Authenticode signed");
  });

  it("supports a pfx thumbprint or Azure Trusted Signing", () => {
    expect(windowsSigning({})).toBeNull();
    expect(windowsSigning({ FORGE_WINDOWS_CERT_THUMBPRINT: "  " })).toBeNull();
    expect(() => windowsSigning({ FORGE_WINDOWS_CERT_THUMBPRINT: "not-hex" })).toThrow(/thumbprint/);
    expect(
      windowsSigning({ FORGE_WINDOWS_CERT_THUMBPRINT: "AB12".repeat(10), FORGE_WINDOWS_TIMESTAMP_URL: "http://ts.test" })
        ?.timestampUrl,
    ).toBe("http://ts.test");
    const azure = {
      FORGE_AZURE_SIGNING_ENDPOINT: "https://weu.codesigning.azure.net",
      FORGE_AZURE_SIGNING_ACCOUNT: "forge",
      FORGE_AZURE_SIGNING_PROFILE: "forge-public",
    };
    expect(windowsSigning(azure)).toEqual({
      signCommand: "trusted-signing-cli -e https://weu.codesigning.azure.net -a forge -c forge-public -d Forge %1",
    });
    expect(() => windowsSigning({ FORGE_AZURE_SIGNING_ACCOUNT: "forge" })).toThrow(/Azure Trusted Signing/);
  });
});
