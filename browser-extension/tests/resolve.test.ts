import { describe, expect, it } from "vitest";
import { angularName, fileFromDebugStack, reactComponentName, resolveElement } from "../src/lib/resolve";

const attach = (el: Element, key: string, value: unknown) => ((el as any)[key] = value);

function dom(html: string): HTMLElement {
  const root = document.createElement("div");
  root.innerHTML = html;
  document.body.replaceChildren(root);
  return root;
}

describe("React", () => {
  function ProductCard() {}
  function App() {}

  it("uses _debugSource of the element and names the owning component", () => {
    const root = dom(`<section><h2 id="t">Lamp</h2></section>`);
    const h2 = root.querySelector("#t")!;
    const appFiber = { type: App, return: null };
    const cardFiber = { type: ProductCard, return: appFiber };
    attach(h2, "__reactFiber$x1y2", {
      type: "h2",
      return: cardFiber,
      _debugSource: { fileName: "/home/me/shop/src/ProductCard.tsx", lineNumber: 14, columnNumber: 7 },
    });
    expect(resolveElement(h2.firstChild!.parentElement!)).toMatchObject({
      framework: "react",
      name: "ProductCard",
      file: "/home/me/shop/src/ProductCard.tsx",
      line: 14,
      column: 7,
      element: h2,
    });
  });

  it("walks up to the nearest element with a fiber", () => {
    const root = dom(`<div id="host"><span><b id="leaf">x</b></span></div>`);
    attach(root.querySelector("#host")!, "__reactInternalInstance$abc", {
      type: "div",
      _debugOwner: { type: { displayName: "LegacyPanel" } },
      _debugSource: { fileName: "src/LegacyPanel.js", lineNumber: 3 },
    });
    expect(resolveElement(root.querySelector("#leaf")!)).toMatchObject({ name: "LegacyPanel", file: "src/LegacyPanel.js", line: 3 });
  });

  it("falls back to the component name and debug stack on React 19", () => {
    const root = dom(`<button id="b">Buy</button>`);
    const stack = `Error: react-stack-top-frame
    at exports.jsxDEV (http://localhost:5173/node_modules/.vite/deps/react_jsx-dev-runtime.js?v=1:250:30)
    at BuyButton (http://localhost:5173/src/BuyButton.tsx?t=17:8:5)`;
    attach(root.querySelector("#b")!, "__reactFiber$r19", {
      type: "button",
      _debugStack: { stack },
      return: { type: function BuyButton() {}, return: null },
    });
    expect(resolveElement(root.querySelector("#b")!)).toMatchObject({
      framework: "react",
      name: "BuyButton",
      file: "http://localhost:5173/src/BuyButton.tsx?t=17",
    });
    expect(resolveElement(root.querySelector("#b")!)?.line).toBeUndefined();
  });

  it("names wrapped components", () => {
    const Inner = function Inner() {};
    expect(reactComponentName({ render: Inner })).toBe("Inner");
    expect(reactComponentName({ type: { displayName: "Memo" } })).toBe("Memo");
    expect(reactComponentName("div")).toBeUndefined();
  });

  it("parses debug stacks", () => {
    expect(fileFromDebugStack("Error\n    at X (webpack-internal:///./src/X.jsx:3:1)")).toBe("webpack-internal:///./src/X.jsx");
    expect(fileFromDebugStack(undefined)).toBeUndefined();
  });
});

describe("Vue", () => {
  it("reads Vue 3 component files and highlights the component root", () => {
    const root = dom(`<aside id="cart"><p id="p">Total</p></aside>`);
    const cart = root.querySelector("#cart")!;
    const instance = {
      type: { __name: "CartSummary", __file: "/home/me/shop/resources/js/CartSummary.vue" },
      subTree: { el: cart },
      parent: null,
    };
    attach(root.querySelector("#p")!, "__vueParentComponent", instance);
    expect(resolveElement(root.querySelector("#p")!)).toMatchObject({
      framework: "vue",
      name: "CartSummary",
      file: "/home/me/shop/resources/js/CartSummary.vue",
      element: cart,
    });
  });

  it("reads Vue 2 options", () => {
    const root = dom(`<div id="vm"><i id="i"></i></div>`);
    const el = root.querySelector("#vm")!;
    attach(el, "__vue__", { $options: { name: "OrderList", __file: "src/components/OrderList.vue" }, $el: el });
    expect(resolveElement(root.querySelector("#i")!)).toMatchObject({
      framework: "vue2",
      name: "OrderList",
      file: "src/components/OrderList.vue",
      element: el,
    });
  });
});

describe("Svelte", () => {
  it("converts Svelte 4 locations to 1-based", () => {
    const root = dom(`<h1 id="h">Hi</h1>`);
    attach(root.querySelector("#h")!, "__svelte_meta", { loc: { file: "src/routes/+page.svelte", line: 4, column: 2, char: 50 } });
    expect(resolveElement(root.querySelector("#h")!)).toMatchObject({
      framework: "svelte",
      name: "+page",
      file: "src/routes/+page.svelte",
      line: 5,
      column: 3,
    });
  });

  it("keeps Svelte 5 lines", () => {
    const root = dom(`<h1 id="h">Hi</h1>`);
    attach(root.querySelector("#h")!, "__svelte_meta", { parent: null, loc: { file: "src/App.svelte", line: 4, column: 0 } });
    expect(resolveElement(root.querySelector("#h")!)).toMatchObject({ line: 4, column: 1 });
  });
});

describe("Angular", () => {
  it("asks the ng dev API for the owning component", () => {
    const root = dom(`<app-user-card id="host"><span id="s">Ada</span></app-user-card>`);
    const host = root.querySelector("#host")!;
    class _UserCardComponent {}
    const component = new _UserCardComponent();
    const ng = {
      getComponent: (el: Element) => (el === host ? component : null),
      getOwningComponent: (el: Element) => (host.contains(el) ? component : null),
      getHostElement: () => host,
    };
    expect(resolveElement(root.querySelector("#s")!, { ng })).toMatchObject({
      framework: "angular",
      name: "UserCardComponent",
      element: host,
    });
    expect(resolveElement(root.querySelector("#s")!, { ng })?.file).toBeUndefined();
  });

  it("strips decorator prefixes from class names", () => {
    expect(angularName(class __AppComponent {})).toBe("AppComponent");
  });
});

describe("source attributes", () => {
  it("prefers explicit attributes on the element", () => {
    const root = dom(`<div data-source-file="src/Card.tsx" data-source-line="9"><b id="b" data-v-inspector="src/Nav.vue:12:5">x</b></div>`);
    expect(resolveElement(root.querySelector("#b")!)).toMatchObject({ framework: "attribute", file: "src/Nav.vue", line: 12, column: 5 });
    expect(resolveElement(root.querySelector("div")!)).toMatchObject({ file: "src/Card.tsx", line: 9 });
  });

  it("reads code-inspector paths with a tag suffix", () => {
    const root = dom(`<p id="p" data-insp-path="/home/me/shop/src/App.tsx:20:7:p">x</p>`);
    expect(resolveElement(root.querySelector("#p")!)).toMatchObject({ file: "/home/me/shop/src/App.tsx", line: 20, column: 7 });
  });

  it("returns null for plain pages", () => {
    const root = dom(`<p id="p">static</p>`);
    expect(resolveElement(root.querySelector("#p")!, {})).toBeNull();
  });
});
