import { describe, expect, it } from "vitest";
import { applyCss, pageUses, plan, sameFile, type LiveEditEvent } from "../src/lib/liveEdit";

const ev = (kind: "css" | "reload", file: string, text?: string, seq = 1): LiveEditEvent => ({
  seq,
  kind,
  file,
  name: file.split("/").pop()!,
  text,
});

function page(html: string): Document {
  document.head.innerHTML = "";
  document.body.innerHTML = "";
  document.head.innerHTML = html;
  return document;
}

describe("live edit", () => {
  it("matches resource paths to project files", () => {
    expect(sameFile("/build/assets/app.css", "public/build/assets/app.css", "app.css")).toBe(true);
    expect(sameFile("/css/app.css", "public/css/app.css", "app.css")).toBe(true);
    expect(sameFile("/css/main.css", "public/css/app.css", "app.css")).toBe(false);
  });

  it("swaps style sheets with unsaved text and reloads them on save", () => {
    const doc = page('<link rel="stylesheet" href="/css/app.css"><link rel="stylesheet" href="/css/other.css">');
    expect(plan(doc, ev("css", "public/css/app.css"))).toBe("css");
    expect(applyCss(doc, ev("css", "public/css/app.css", "h1 { color: red }"))).toBe(1);
    const link = doc.querySelector<HTMLLinkElement>('link[href="/css/app.css"]')!;
    expect(link.disabled).toBe(true);
    expect(doc.querySelector("style[data-forge-live]")?.textContent).toBe("h1 { color: red }");
    // Typing again updates the same <style>.
    applyCss(doc, ev("css", "public/css/app.css", "h1 { color: blue }", 2));
    expect(doc.querySelectorAll("style[data-forge-live]").length).toBe(1);
    // Saved: the link comes back, freshly loaded.
    applyCss(doc, ev("css", "public/css/app.css", undefined, 3));
    expect(doc.querySelector("style[data-forge-live]")).toBeNull();
    expect(link.disabled).toBe(false);
    expect(link.getAttribute("href")).toContain("forge-live=3");
  });

  it("reloads pages that loaded a changed file", () => {
    const doc = page('<script src="/assets/main.js"></script>');
    expect(pageUses(doc, ev("reload", "src/main.js"))).toBe(false);
    expect(plan(doc, ev("reload", "public/assets/main.js"))).toBe("reload");
    expect(plan(doc, ev("reload", "src/Other.vue"), ["http://localhost:5173/src/Other.vue?t=1"])).toBe("reload");
    expect(plan(doc, ev("css", "src/styles/app.css"))).toBe(null);
  });
});
