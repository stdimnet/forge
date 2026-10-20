import { describe, expect, it } from "vitest";
import { classifyDql, dqlSourceSpans, findDqlSites, sourceToDql, tokenizeDql } from "./dql";

const kinds = (dql: string) => classifyDql(dql).map((s) => [dql.slice(s.from, s.to), s.kind]);

describe("DQL tokenizer", () => {
  it("separates keywords, functions, paths and parameters", () => {
    const t = tokenizeDql("SELECT COUNT(o.id) FROM App\\Entity\\Order o WHERE o.status = :s").filter((x) => x.kind !== "whitespace");
    expect(t.map((x) => x.kind)).toEqual([
      "keyword", "function", "lparen", "identifier", "rparen", "keyword", "identifier", "identifier", "keyword", "identifier", "operator", "parameter",
    ]);
  });

  it("treats a keyword-like entity name after FROM as identifier", () => {
    const t = tokenizeDql("FROM Order o").filter((x) => x.kind !== "whitespace");
    expect(t[1].kind).toBe("identifier");
  });
});

describe("classifyDql", () => {
  it("marks entities, aliases and fields", () => {
    expect(kinds("SELECT o FROM App\\Entity\\Order o JOIN o.customer c WHERE c.email = :e")).toEqual([
      ["SELECT", "keyword"],
      ["o", "alias"],
      ["FROM", "keyword"],
      ["App\\Entity\\Order", "entity"],
      ["o", "alias"],
      ["JOIN", "keyword"],
      ["o", "alias"],
      ["customer", "field"],
      ["c", "alias"],
      ["WHERE", "keyword"],
      ["c", "alias"],
      ["email", "field"],
      [":e", "parameter"],
    ]);
  });

  it("colors fragment paths without declarations", () => {
    expect(kinds("o.status = :status")).toEqual([
      ["o", "alias"],
      ["status", "field"],
      [":status", "parameter"],
    ]);
  });
});

describe("findDqlSites", () => {
  const src = `<?php
class R {
    function f() {
        $qb = $this->createQueryBuilder('o')
            ->join('o.customer', 'c')
            ->andWhere('o.status = :status')
            ->orderBy('o.paidAt', 'DESC');
        $this->render('order/show.html.twig');
        return $this->getEntityManager()->createQuery(
            'SELECT o FROM ' . Order::class . ' o
             WHERE o.total > 10'
        );
    }
}
`;

  it("finds createQuery and QueryBuilder arguments", () => {
    const sites = findDqlSites(src);
    expect(sites.map((s) => [s.method, s.mode, s.dql])).toEqual([
      ["join", "join", "o.customer"],
      ["andWhere", "expression", "o.status = :status"],
      ["orderBy", "expression", "o.paidAt"],
      ["createQuery", "full", "SELECT o FROM Order o\n             WHERE o.total > 10"],
    ]);
  });

  it("maps source offsets into the DQL", () => {
    const site = findDqlSites(src).find((s) => s.method === "andWhere")!;
    const pos = src.indexOf("o.status = :status") + 2;
    expect(sourceToDql(site, pos)).toBe(2);
    expect(sourceToDql(site, src.indexOf("order/show"))).toBeNull();
  });

  it("produces spans in source offsets", () => {
    const { spans } = dqlSourceSpans(src);
    const text = spans.map((s) => src.slice(s.from, s.to));
    expect(text).toContain("paidAt");
    expect(text).toContain("SELECT");
    expect(text).not.toContain("DESC");
  });
});
