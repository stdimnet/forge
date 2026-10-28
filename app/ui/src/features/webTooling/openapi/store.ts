/**
 * OpenAPI documents of the open project (`openapi_endpoints`, read in Rust), the paths
 * known to be OpenAPI files (they open with the preview), and "Generate HTTP Request".
 */
import { createSignal } from "solid-js";
import { openApiApi, type OpenApiFile } from "@/ipc/webTooling";
import { openHttpRequest } from "@/features/scratches/store";
import { editor } from "@/state/editor";
import { workspaceApi } from "@/ipc/workspace";
import { balloonsFor } from "@/keymap";

const showBalloon = balloonsFor("OpenAPI");

const [files, setFiles] = createSignal<OpenApiFile[]>([]);
const [loading, setLoading] = createSignal(false);
export { files as openApiFiles, loading as openApiLoading };

/** OpenAPI files by path; also those confirmed by the preview after an edit. */
const [known, setKnown] = createSignal<Set<string>>(new Set());

export const isOpenApiPath = (path: string) =>
  known().has(path) || (/(^|\/)(openapi|swagger)[\w.-]*\.(ya?ml|json)$/i.test(path) && !path.includes("://"));

export function markOpenApi(path: string, yes: boolean): void {
  setKnown((s) => {
    if (s.has(path) === yes) return s;
    const n = new Set(s);
    if (yes) n.add(path);
    else n.delete(path);
    return n;
  });
}

export async function loadOpenApi(): Promise<void> {
  setLoading(true);
  try {
    const list = await openApiApi.endpoints();
    setFiles(list);
    setKnown(new Set(list.map((f) => f.path)));
  } catch {
    setFiles([]);
  } finally {
    setLoading(false);
  }
}

async function contentOf(path: string): Promise<string> {
  const tab = editor.tabs.find((t) => t.path === path);
  return tab ? tab.content : workspaceApi.readFile(path);
}

/** Generate HTTP Request: a new HTTP scratch with the operation's request. */
export async function generateHttpRequest(path: string, method: string, opPath: string): Promise<void> {
  const text = await openApiApi.httpRequest(path, await contentOf(path), method, opPath).catch(() => null);
  if (!text) return void showBalloon("Cannot generate a request", `${method} ${opPath} was not found in ${path}.`);
  await openHttpRequest(text);
}
