import { invoke } from "./index";

export interface FileMatch {
  path: string;
  name: string;
  score: number;
  /** Highlight positions into `name`, or into `path` when `matchedPath`. */
  positions: number[];
  matchedPath: boolean;
}

export const searchApi = {
  /** Fuzzy "Go to File" over the project (respects .gitignore). */
  findFiles: (query: string, limit = 100) => invoke<FileMatch[]>("fs_find_files", { query, limit }),
};
