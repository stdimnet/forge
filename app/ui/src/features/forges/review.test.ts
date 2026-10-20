import { describe, expect, it } from "vitest";
import type { ChangedFile, Discussion } from "@/ipc/forges";
import { diffRows, threadOnRow } from "./ReviewDiff";
import { highlightLines, parserFor } from "./highlight";

const file: ChangedFile = {
  path: "app/Models/User.php",
  oldPath: null,
  status: "modified",
  additions: 2,
  deletions: 1,
  hunks: [
    {
      oldStart: 10,
      oldLines: 3,
      newStart: 10,
      newLines: 4,
      header: "class User",
      lines: [
        { kind: "context", text: "    use HasFactory;" },
        { kind: "removed", text: "    protected $guarded = [];" },
        { kind: "added", text: "    protected $fillable = ['name'];" },
        { kind: "added", text: "    protected $hidden = ['password'];" },
        { kind: "context", text: "}" },
      ],
    },
  ],
};

const thread = (newLine: number | null, oldLine: number | null): Discussion => ({
  id: "t",
  notes: [],
  position: { path: file.path, newLine, oldLine },
  resolvable: true,
  resolved: false,
});

describe("review diff", () => {
  it("numbers old and new lines", () => {
    const rows = diffRows(file);
    expect(rows.map((r) => [r.kind, r.oldNo, r.newNo])).toEqual([
      ["hunk", null, null],
      ["context", 10, 10],
      ["removed", 11, null],
      ["added", null, 11],
      ["added", null, 12],
      ["context", 12, 13],
    ]);
  });

  it("places threads on new lines, old lines of removed rows and context rows", () => {
    const rows = diffRows(file);
    const at = (d: Discussion) => rows.findIndex((r) => threadOnRow(d, r));
    expect(at(thread(12, null))).toBe(4);
    expect(at(thread(null, 11))).toBe(2);
    expect(at(thread(null, 12))).toBe(5);
    expect(at(thread(99, null))).toBe(-1);
  });

  it("highlights PHP fragments without an opening tag", () => {
    const [line] = highlightLines(["    protected $hidden = ['password'];"], parserFor("a.php"));
    expect(line.map((s) => s.text).join("")).toBe("    protected $hidden = ['password'];");
    expect(line.some((s) => s.text === "$hidden" && s.cls)).toBe(true);
    expect(line.some((s) => s.text === "protected" && s.cls)).toBe(true);
  });
});
