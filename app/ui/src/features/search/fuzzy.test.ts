import { describe, expect, it } from "vitest";
import { fuzzyFilter, fuzzyMatch, matchFilePath } from "./fuzzy";

describe("fuzzyMatch", () => {
  it("matches camel humps", () => {
    expect(fuzzyMatch("OrCo", "OrderController")?.positions).toEqual([0, 1, 5, 6]);
    expect(fuzzyMatch("OC", "OrderController")?.positions).toEqual([0, 5]);
  });

  it("is case-insensitive", () => {
    expect(fuzzyMatch("orderc", "OrderController")).not.toBeNull();
  });

  it("returns null when characters are missing or out of order", () => {
    expect(fuzzyMatch("xyz", "OrderController")).toBeNull();
    expect(fuzzyMatch("rO", "Order")).toBeNull();
    expect(fuzzyMatch("toolong", "short")).toBeNull();
  });

  it("matches everything for an empty query", () => {
    expect(fuzzyMatch("", "Anything")).toEqual({ score: 0, positions: [] });
  });

  it("ignores whitespace in the query", () => {
    expect(fuzzyMatch("order con", "OrderController")).not.toBeNull();
  });

  it("ranks humps above scattered letters", () => {
    expect(fuzzyMatch("OC", "OrderController")!.score).toBeGreaterThan(fuzzyMatch("OC", "Obscure")!.score);
  });

  it("ranks prefixes above infixes", () => {
    expect(fuzzyMatch("user", "User.php")!.score).toBeGreaterThan(fuzzyMatch("user", "AbuserLog.php")!.score);
  });

  it("treats separators as word starts", () => {
    expect(fuzzyMatch("cot", "create_orders_table")?.positions).toEqual([0, 7, 14]);
  });
});

describe("strict mode", () => {
  it("only jumps to word starts", () => {
    expect(fuzzyMatch("line", "Move Line Up", { strict: true })).not.toBeNull();
    expect(fuzzyMatch("line", "Collapse Recursively", { strict: true })).toBeNull();
    expect(fuzzyMatch("line", "Add Selection for Next Occurrence", { strict: true })).toBeNull();
    expect(fuzzyMatch("OrCo", "OrderController", { strict: true })?.positions).toEqual([0, 1, 5, 6]);
  });

  it("still allows a match to start mid-word", () => {
    expect(fuzzyMatch("ine", "Line", { strict: true })?.positions).toEqual([1, 2, 3]);
  });
});

describe("fuzzyFilter", () => {
  const classes = ["OrderController", "Controller", "Order", "OrderStatus", "UserController"];

  it("orders by score and drops non-matches", () => {
    const res = fuzzyFilter("OrCo", classes, (s) => s).map((r) => r.item);
    expect(res[0]).toBe("OrderController");
    expect(res).not.toContain("Order");
  });

  it("prefers shorter candidates on equal score", () => {
    expect(fuzzyFilter("order", classes, (s) => s)[0].item).toBe("Order");
  });

  it("respects the limit", () => {
    expect(fuzzyFilter("o", classes, (s) => s, 2)).toHaveLength(2);
  });
});

describe("matchFilePath", () => {
  it("matches file names by default", () => {
    const hit = matchFilePath("OrCo", "app/Http/Controllers/OrderController.php");
    expect(hit?.matchedPath).toBe(false);
    expect(hit?.positions).toEqual([0, 1, 5, 6]);
    expect(matchFilePath("Http", "app/Http/Controllers/OrderController.php")).toBeNull();
  });

  it("matches whole paths when the query has a slash", () => {
    const hit = matchFilePath("http/ord", "app/Http/Controllers/OrderController.php");
    expect(hit?.matchedPath).toBe(true);
  });
});
