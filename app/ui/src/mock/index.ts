/** Browser-mode backend. Features add their own `*Mock.ts` imports here. */
import "./settingsMock";
import "./keymapMock";
import { fileTypesReady } from "./filetypesMock";
import "./projectMock";
import "./remoteMock";
import "./workspaceMock";
import "./aiMock";
import "./agentMock";
import "./lspMock";
import "./langserversMock";
import "./fuzzyMock";
import "./searchMock";
import "./findMock";
import "./replaceMock";
import "./workbenchMock";
import "./gitMock";
import "./phpMock";
import "./codeintelMock";
import "./companionMock";
import "./terminalMock";
import "./runMock";
import "./scratchesMock";
import "./debugMock";
import "./dbMock";
import "./laravelMock";
import "./symfonyMock";
import "./luaMock";
import "./doctrineMock";
import "./forgesMock";
import "./markdownMock";
import "./servicesMock";
import "./webMock";
import "./windowMock";
import "./updatesMock";

// File types answer synchronously once the WebAssembly build is loaded; wait for it so the
// first editors do not start as plain text.
await fileTypesReady;
