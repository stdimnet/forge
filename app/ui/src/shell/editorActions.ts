import { openFile } from "@/state/editor";

export const setEditorActive = (path: string) => void openFile(path);
