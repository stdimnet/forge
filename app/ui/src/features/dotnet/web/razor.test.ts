import { describe, expect, it } from "vitest";
import { csharpTokens, openTag, scanRazor, tagAt } from "./razor";

const kinds = (text: string) => scanRazor(text).map((r) => `${r.kind}:${text.slice(r.from, r.to)}`);

describe("Razor scanner", () => {
  it("finds expressions, blocks, directives and comments", () => {
    expect(kinds('@page "/shelf"\n<h1>@Model.Title</h1>')).toEqual(["directive:@page", 'csharp: "/shelf"', "transition:@", "csharp:Model.Title"]);
    expect(kinds("<p>@(a + b) and @@ and anna@example.com</p>")).toEqual(["transition:@", "csharp:(a + b)"]);
    expect(kinds("@* note *@<b>x</b>")).toEqual(["comment:@* note *@"]);
    expect(kinds("@code {\n  int count;\n}")).toEqual(["directive:@code", "csharp:{\n  int count;\n}"]);
    expect(kinds('<button @onclick="Increment">+</button>')).toEqual(["attribute:@onclick", "csharp:Increment"]);
    expect(kinds("@Format(x)[0].Name rest")).toEqual(["transition:@", "csharp:Format(x)[0].Name"]);
  });

  it("keeps markup inside control blocks", () => {
    const k = kinds("@if (ok)\n{\n  <p>@name</p>\n}\nelse\n{\n  <i>no</i>\n}");
    expect(k).toEqual(["transition:@", "csharp:if", "csharp:(ok)", "csharp:{", "transition:@", "csharp:name", "csharp:}", "csharp:\nelse", "csharp:{", "csharp:}"]);
  });

  it("colors C# inside regions", () => {
    const text = "@{ var n = 3; }";
    const r = scanRazor(text).find((x) => x.kind === "csharp")!;
    const styles = csharpTokens(text, r.from, r.to).map((t) => `${t.style}:${text.slice(t.from, t.to)}`);
    expect(styles).toContain("keyword:var");
    expect(styles).toContain("number:3");
  });

  it("finds component tags", () => {
    expect(tagAt("  <Rating Value=\"3\" />", 5)).toBe("Rating");
    expect(tagAt("</Shared.Rating>", 10)).toBe("Rating");
    expect(tagAt("<div>", 2)).toBeNull();
    expect(openTag('<Rating Value="3" ')).toEqual({ name: "Rating", attrs: true });
    expect(openTag("<Rat")).toEqual({ name: "Rat", attrs: false });
    expect(openTag("<Rating /> x")).toBeNull();
  });
});
