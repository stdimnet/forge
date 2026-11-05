/** Types of validate.mjs for TypeScript callers (the SDK tests in app/ui). */
export declare function isForge(file: string | undefined): boolean;
export declare function findForge(opts?: { env?: Record<string, string | undefined>; repo?: string }): string | null;
export declare function basicChecks(dir: string): string[];
export declare function validate(
  args: string[],
  opts?: { repo?: string; log?: (msg: string) => void; error?: (msg: string) => void },
): number;
