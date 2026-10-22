import { describe, expect, it } from "vitest";
import { columnTitles, delimiterName, minimalChange } from "./model";

describe("csv table model", () => {
  it("titles columns from the header or C1, C2, …", () => {
    expect(columnTitles(["id", "", "id"], 4)).toEqual(["id", "C2", "id (2)", "C4"]);
    expect(columnTitles(undefined, 2)).toEqual(["C1", "C2"]);
  });

  it("finds the smallest replacement", () => {
    expect(minimalChange("a,b\n1,2\n", "a,b\n1,3\n")).toEqual({ from: 6, to: 7, insert: "3" });
    expect(minimalChange("x", "x")).toBeNull();
    expect(minimalChange("a\n", "a\nb\n")).toEqual({ from: 2, to: 2, insert: "b\n" });
  });

  it("names delimiters", () => {
    expect(delimiterName("\t")).toBe("tab");
    expect(delimiterName(":")).toBe('":"');
  });
});
