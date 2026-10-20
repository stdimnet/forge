/*
 * Renders the demo shop and attaches the same development internals React 18 and Vue 3 put
 * on DOM nodes (`__reactFiber$…` with `_debugSource`, `__vueParentComponent`), so the
 * extension can be tried without a real dev server. Paths point into a Laravel + React
 * project at /home/me/Projects/acme-shop.
 */
const ROOT = "/home/me/Projects/acme-shop/resources/js";
const FIBER = "__reactFiber$demo4f2k";

function component(name) {
  const fn = function () {};
  Object.defineProperty(fn, "name", { value: name });
  return fn;
}

const App = { type: component("ShopPage"), return: null };
const owners = {};
function owner(name) {
  return (owners[name] ??= { type: component(name), return: App });
}

/** Attach a host fiber: `file` and `line` are where the JSX element was written. */
function fiber(el, componentName, file, line, column = 7) {
  el[FIBER] = {
    type: el.tagName.toLowerCase(),
    return: owner(componentName),
    _debugSource: { fileName: `${ROOT}/${file}`, lineNumber: line, columnNumber: column },
  };
}

const products = [
  { name: "Forge Lamp", meta: "Brass · 40 cm", price: "€89.00", color: "#fde8d7", stroke: "#e8620c" },
  { name: "Anvil Desk", meta: "Oak · 140 × 70 cm", price: "€640.00", color: "#e2e9fb", stroke: "#2b59c3" },
  { name: "Ember Mug", meta: "Stoneware · 350 ml", price: "€19.00", color: "#f6e3e3", stroke: "#c43f3f" },
  { name: "Bellows Chair", meta: "Leather · walnut", price: "€420.00", color: "#e3f1e6", stroke: "#2f8a46" },
  { name: "Tongs Hook Set", meta: "Steel · set of 4", price: "€24.00", color: "#efe9f9", stroke: "#7a3fc4" },
  { name: "Hearth Rug", meta: "Wool · 200 × 140 cm", price: "€210.00", color: "#fbf1d6", stroke: "#b58a00" },
];

const grid = document.getElementById("grid");
fiber(grid, "ProductGrid", "components/ProductGrid.tsx", 21, 5);
for (const p of products) {
  const card = document.createElement("article");
  card.className = "card";
  card.innerHTML = `
    <div class="thumb" style="background:${p.color}">
      <svg viewBox="0 0 64 64" fill="none" stroke="${p.stroke}" stroke-width="4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 44h40M18 44l4-20h20l4 20M26 24v-8h12v8"/>
      </svg>
    </div>
    <div class="card-body">
      <h3>${p.name}</h3>
      <div class="meta">${p.meta}</div>
      <div class="row"><span class="price">${p.price}</span><button class="add">Add to cart</button></div>
    </div>`;
  grid.appendChild(card);
  fiber(card, "ProductCard", "components/ProductCard.tsx", 14, 5);
  fiber(card.querySelector(".thumb"), "ProductCard", "components/ProductCard.tsx", 15);
  fiber(card.querySelector("svg"), "ProductThumb", "components/ProductThumb.tsx", 9);
  fiber(card.querySelector(".card-body"), "ProductCard", "components/ProductCard.tsx", 18);
  fiber(card.querySelector("h3"), "ProductCard", "components/ProductCard.tsx", 19, 9);
  fiber(card.querySelector(".meta"), "ProductCard", "components/ProductCard.tsx", 20, 9);
  fiber(card.querySelector(".row"), "ProductCard", "components/ProductCard.tsx", 21, 9);
  fiber(card.querySelector(".price"), "Price", "components/Price.tsx", 6);
  fiber(card.querySelector(".add"), "AddToCartButton", "components/AddToCartButton.tsx", 12);
}

const header = document.getElementById("site-header");
fiber(header, "SiteHeader", "components/SiteHeader.tsx", 9, 5);
for (const el of header.querySelectorAll("nav, nav a, .brand, .cart-btn")) {
  fiber(el, "SiteHeader", "components/SiteHeader.tsx", el.matches("nav a") ? 14 : el.matches("nav") ? 13 : el.matches(".brand") ? 10 : 18);
}
for (const el of document.querySelectorAll("main, section, h1, .lead")) {
  fiber(el, "ShopPage", "pages/Shop.tsx", el.matches("h1") ? 31 : el.matches(".lead") ? 32 : 28);
}

// The cart is a Vue 3 island (as in many Laravel apps).
const cart = document.getElementById("cart");
const cartInstance = {
  type: { __name: "CartSummary", __file: `${ROOT}/components/CartSummary.vue` },
  subTree: { el: cart },
  parent: null,
};
for (const el of [cart, ...cart.querySelectorAll("*")]) el.__vueParentComponent = cartInstance;
