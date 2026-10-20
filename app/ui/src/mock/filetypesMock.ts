/**
 * Browser-mode file types: the same Rust detection as the desktop app (`forge-filetypes`),
 * run as WebAssembly over the built-in plugin manifests. Answers synchronously, so editors
 * never wait for a file type here.
 */
import { registerMock } from "@/ipc";
import type { DetectRequest, FileTypeInfo } from "@/ipc/filetypes";
import { setSyncFileTypeResolver } from "@/editor/fileTypes";
import { callWasmSync, forgeWasm } from "@/wasm";
import builtinPlugins from "./builtinPlugins.json";

const wasm = await forgeWasm();
callWasmSync<number>(wasm.filetypes_load, builtinPlugins);

const detect = (request: DetectRequest) => callWasmSync<FileTypeInfo[]>(wasm.filetypes_detect, request);

registerMock("filetypes_detect", ({ request }) => detect(request as DetectRequest));
setSyncFileTypeResolver(detect);
