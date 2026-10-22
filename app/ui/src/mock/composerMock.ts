/**
 * Browser-mode Composer backend. Browser mode has no network: a small demo catalog stands
 * in for Packagist and the demo project gets a composer.lock; markers, completion, hover
 * and the audit problems are computed by the Rust `phpforge-composer` crate (WebAssembly),
 * exactly like in the desktop app. Composer itself never runs here (see `runMock.ts`).
 */
import { registerMock } from "@/ipc";
import type { AuditResult, PackageDetails, SearchHit } from "@/ipc/composer";
import { callWasm, forgeWasm } from "@/wasm";
import { mockSettingValue } from "./settingsMock";
import { DEMO_CATALOG } from "./demoComposer";
import { mockFiles } from "./workspaceMock";

const catalog = () => DEMO_CATALOG.map(({ downloads: _d, favers: _f, ...p }) => p);
const jsonPath = () => String(mockSettingValue("php.composer.json") || "composer.json");
const lockPath = (json: string) => (json.includes("/") ? `${json.slice(0, json.lastIndexOf("/"))}/composer.lock` : "composer.lock");
const packagistOn = () => mockSettingValue("php.composer.packagist") !== false;
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const offline = () => new Error("Packagist access is turned off (Settings | Languages & Frameworks | PHP | Composer)");

registerMock("composer_project", async () => {
  const w = await forgeWasm();
  const json = jsonPath();
  const text = mockFiles[json];
  const project = await callWasm<object>(w.composer_project, { text: text ?? "{}", lock: mockFiles[lockPath(json)] });
  return { ...project, composerJson: json, exists: text !== undefined };
});

registerMock("composer_markers", async ({ path, text }) => {
  if (!packagistOn()) return [];
  const w = await forgeWasm();
  return callWasm(w.composer_markers, { text, lock: mockFiles[lockPath(String(path))], catalog: catalog() });
});

registerMock("composer_complete", async ({ path, text, offset }) => {
  const w = await forgeWasm();
  return callWasm(w.composer_complete, {
    text,
    offset,
    lock: mockFiles[lockPath(String(path))],
    catalog: packagistOn() ? catalog() : [],
  });
});

registerMock("composer_hover", async ({ path, text, offset }) => {
  const w = await forgeWasm();
  return callWasm(w.composer_hover, {
    text,
    offset,
    lock: mockFiles[lockPath(String(path))],
    catalog: packagistOn() ? catalog() : [],
  });
});

registerMock("composer_search", async ({ query }) => {
  if (!packagistOn()) throw offline();
  await delay(150);
  const w = await forgeWasm();
  const hits = await callWasm<SearchHit[]>(w.composer_search, { query, catalog: catalog() });
  return hits
    .map((h) => {
      const p = DEMO_CATALOG.find((c) => c.name === h.name)!;
      return { ...h, downloads: p.downloads, favers: p.favers };
    })
    .sort((a, b) => b.downloads - a.downloads);
});

/** What `composer require` would write for the latest stable version (demo data). */
const SUGGESTED: Record<string, string> = {
  "laravel/framework": "^12.3",
  "laravel/sanctum": "^4.0",
  "laravel/horizon": "^5.31",
  "laravel/telescope": "^5.5",
  "laravel/pint": "^1.21",
  "doctrine/dbal": "^4.2",
  "doctrine/orm": "^3.3",
  "guzzlehttp/guzzle": "^7.9",
  "guzzlehttp/psr7": "^2.7",
  "phpunit/phpunit": "^12.0",
  "pestphp/pest": "^3.7",
  "spatie/laravel-permission": "^6.16",
  "monolog/monolog": "^3.9",
  "nesbot/carbon": "^3.8",
  "swiftmailer/swiftmailer": "^6.3",
};

registerMock("composer_package", async ({ name }): Promise<PackageDetails> => {
  if (!packagistOn()) throw offline();
  await delay(80);
  const p = DEMO_CATALOG.find((c) => c.name === name);
  if (!p) throw new Error("not found on Packagist");
  const { downloads: _d, favers: _f, ...info } = p;
  return { ...info, suggested: SUGGESTED[p.name] ?? null, url: `https://packagist.org/packages/${p.name}` };
});

/** `composer audit --format=json` of the demo lock file. */
export const DEMO_AUDIT = {
  advisories: [
    {
      advisoryId: "PKSA-8qx3-n5y5-vvnd",
      packageName: "guzzlehttp/psr7",
      affectedVersions: ">=2,<2.4.5|<1.9.1",
      title: "Improper header validation",
      cve: "CVE-2023-29197",
      link: "https://github.com/guzzle/psr7/security/advisories/GHSA-wxmh-65f7-jcvw",
      severity: "medium",
      reportedAt: "2023-04-17T16:00:00+00:00",
      sources: ["GHSA-wxmh-65f7-jcvw"],
    },
    {
      advisoryId: "PKSA-yhcn-xrg3-68b1",
      packageName: "laravel/framework",
      affectedVersions: ">=12.0.0,<12.1.1",
      title: "File validation bypass with uppercase extensions",
      cve: "CVE-2025-27515",
      link: "https://github.com/laravel/framework/security/advisories/GHSA-78fx-h6xr-vch4",
      severity: "medium",
      reportedAt: "2025-03-05T18:00:00+00:00",
      sources: ["GHSA-78fx-h6xr-vch4"],
    },
  ],
  abandoned: [],
};

registerMock("composer_audit", async (): Promise<AuditResult> => {
  await delay(400);
  const w = await forgeWasm();
  const json = jsonPath();
  const lockFile = lockPath(json);
  const problems = await callWasm<Pick<AuditResult, "jsonProblems" | "lockProblems">>(w.composer_audit_problems, {
    report: DEMO_AUDIT,
    text: mockFiles[json] ?? "{}",
    lock: mockFiles[lockFile],
  });
  return { ...DEMO_AUDIT, composerJson: json, composerLock: lockFile, ...problems };
});

registerMock("composer_download_phar", async () => {
  await delay(600);
  return "~/.local/share/forge/composer/composer.phar";
});
