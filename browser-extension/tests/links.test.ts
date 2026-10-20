import { describe, expect, it } from "vitest";
import { findFileReferences, firstFileReference } from "../src/lib/links";

describe("findFileReferences", () => {
  it("reads JavaScript stack traces", () => {
    const stack = `TypeError: x is undefined
    at ProductCard (http://localhost:5173/src/components/ProductCard.tsx?t=1712:14:9)
    at renderWithHooks (http://localhost:5173/node_modules/.vite/deps/react-dom.js?v=1:1234:22)`;
    expect(findFileReferences(stack)).toEqual([
      { file: "http://localhost:5173/src/components/ProductCard.tsx", line: 14, column: 9 },
      { file: "http://localhost:5173/node_modules/.vite/deps/react-dom.js", line: 1234, column: 22 },
    ]);
  });

  it("reads PHP errors and traces", () => {
    expect(findFileReferences("Fatal error: Uncaught Error in /var/www/app/Models/User.php on line 42")).toEqual([
      { file: "/var/www/app/Models/User.php", line: 42 },
    ]);
    expect(findFileReferences("#0 /var/www/vendor/laravel/framework/src/Foundation/Application.php(1125): run()")).toEqual([
      { file: "/var/www/vendor/laravel/framework/src/Foundation/Application.php", line: 1125, column: undefined },
    ]);
  });

  it("reads compiler output and plain references", () => {
    expect(findFileReferences("src/app/app.component.ts(12,5): error TS2322")).toEqual([
      { file: "src/app/app.component.ts", line: 12, column: 5 },
    ]);
    expect(findFileReferences("see C:\\work\\shop\\src\\a.ts:7 and ./lib/b.js:3:1")).toEqual([
      { file: "C:\\work\\shop\\src\\a.ts", line: 7, column: undefined },
      { file: "./lib/b.js", line: 3, column: 1 },
    ]);
    expect(findFileReferences('File "/srv/app/main.py", line 12, in <module>')).toEqual([
      { file: "/srv/app/main.py", line: 12 },
    ]);
  });

  it("ignores text without references", () => {
    expect(findFileReferences("Version 1.2.3 released at 12:30")).toEqual([]);
  });
});

describe("firstFileReference", () => {
  it("accepts bare paths and GitHub-style #L links", () => {
    expect(firstFileReference("  src/App.tsx ")).toEqual({ file: "src/App.tsx" });
    expect(firstFileReference("https://example.test/src/App.tsx#L12")).toEqual({
      file: "https://example.test/src/App.tsx",
      line: 12,
      column: undefined,
    });
    expect(firstFileReference("hello world")).toBeNull();
  });
});
