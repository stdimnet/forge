/** Types of pack.mjs for TypeScript callers (the SDK tests in app/ui). */
export declare const DOCUMENTS: string[];
export declare function zip(entries: [string, Uint8Array][]): Uint8Array;
export declare function pack(dir: string, out?: string): { file: string; sha256: string; size: number };
