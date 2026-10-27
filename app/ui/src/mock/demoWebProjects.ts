/**
 * WebForge demo projects for browser mode: `?project=react` (a Vite + React storefront in a
 * pnpm workspace), `?project=vue` (a Vue 3 dashboard with yarn) and `?project=angular` (an
 * Angular admin portal with npm) and `?project=node` (a Node.js + TypeScript API with Vitest
 * and Playwright tests, for the JavaScript debugger and the test runners). The default demo
 * is the Laravel shop in demoProject.ts.
 */
import type { ProjectInfo } from "@/ipc/project";

export type WebDemoId = "react" | "vue" | "angular" | "node";

export interface WebDemo {
  name: string;
  /** File opened when the demo starts. */
  initialFile: string;
  info: ProjectInfo;
  files: Record<string, string>;
}

/** The WebForge demo the page asks for, if any. */
export function webDemoId(): WebDemoId | null {
  if (typeof location === "undefined") return null;
  const p = new URLSearchParams(location.search).get("project");
  return p === "react" || p === "vue" || p === "angular" || p === "node" ? p : null;
}

const fw = (id: string, name: string, version: string, language = "javascript") => ({ id, name, version, language });

// ---- React: storefront-ui ---------------------------------------------------------------

const react: Record<string, string> = {
  "package.json": `{
  "name": "storefront-ui",
  "version": "1.4.0",
  "private": true,
  "type": "module",
  "packageManager": "pnpm@9.15.4",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview",
    "lint": "eslint .",
    "typecheck": "tsc -b --noEmit",
    "test": "vitest run",
    "test:watch": "vitest",
    "storybook": "storybook dev -p 6006"
  },
  "dependencies": {
    "react": "^19.1.0",
    "react-dom": "^19.1.0",
    "react-router-dom": "^7.5.3"
  },
  "devDependencies": {
    "@vitejs/plugin-react": "^4.4.1",
    "eslint": "^9.25.0",
    "prettier": "^3.5.3",
    "typescript": "~5.8.3",
    "vite": "^6.3.5",
    "vitest": "^3.1.2"
  }
}
`,
  "pnpm-lock.yaml": "lockfileVersion: '9.0'\n\nsettings:\n  autoInstallPeers: true\n  excludeLinksFromLockfile: false\n",
  "pnpm-workspace.yaml": "packages:\n  - 'packages/*'\n",
  "eslint.config.js": `import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(js.configs.recommended, ...tseslint.configs.recommended, {
  rules: { eqeqeq: "warn", "no-console": ["warn", { allow: ["warn", "error"] }] },
});
`,
  ".prettierrc": '{\n  "printWidth": 120\n}\n',
  ".prettierignore": "dist\npnpm-lock.yaml\n",
  "src/utils/cartTotals.ts": `import type { CartItem } from "@/types";

export function cartTotal(items: CartItem[]): number {
  var total = 0;
  for (const item of items) total += item.price * item.quantity;
  if (items.length == 0) console.log("empty cart");
  return Math.round(total * 100) / 100;
}

export const formatTotal = ( total: number )  =>  \`\${total.toFixed(2)} €\` ;
`,
  "packages/icons/package.json": `{
  "name": "@storefront/icons",
  "version": "0.3.0",
  "scripts": {
    "build": "svgr --typescript --out-dir dist svg",
    "clean": "rm -rf dist"
  }
}
`,
  "packages/icons/svg/cart.svg": '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="9" cy="21" r="1"/></svg>\n',
  "tsconfig.json": `{
  "compilerOptions": {
    "target": "ES2022",
    "jsx": "react-jsx",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "baseUrl": ".",
    "paths": { "@/*": ["src/*"] }
  },
  "include": ["src"]
}
`,
  "vite.config.ts": `import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  plugins: [react()],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
});
`,
  "index.html": `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <title>Storefront</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
`,
  ".gitignore": "node_modules\ndist\n",
  "src/main.tsx": `import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles/app.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
`,
  "src/App.tsx": `import { BrowserRouter, Route, Routes } from "react-router-dom";
import { Header } from "./components/Header";
import { CartDrawer } from "./components/CartDrawer";
import HomePage from "./pages/HomePage";
import { useCart } from "@/hooks/useCart";

export function App() {
  const cart = useCart();

  return (
    <BrowserRouter>
      <Header title="Storefront" cartCount={cart.items.length} onOpenCart={cart.open} />
      <main className="page">
        <Routes>
          <Route path="/" element={<HomePage onAddToCart={cart.add} />} />
        </Routes>
      </main>
      <CartDrawer open={cart.isOpen} items={cart.items} onClose={cart.close} />
    </BrowserRouter>
  );
}
`,
  "src/components/Header.tsx": `import { Badge, Button } from "@/components/ui";

interface HeaderProps {
  title: string;
  cartCount: number;
  onOpenCart: () => void;
}

export function Header({ title, cartCount, onOpenCart }: HeaderProps) {
  return (
    <header className="header">
      <h1>{title}</h1>
      <Button variant="ghost" onClick={onOpenCart}>
        Cart <Badge tone="accent">{cartCount}</Badge>
      </Button>
    </header>
  );
}
`,
  "src/components/ProductCard.tsx": `import { memo } from "react";
import { Badge, Button } from "@/components/ui";
import type { Product } from "@/types";

type ProductCardProps = {
  product: Product;
  onAddToCart: (id: string) => void;
  compact?: boolean;
};

function ProductCard({ product, onAddToCart, compact = false }: ProductCardProps) {
  return (
    <article className={compact ? "card compact" : "card"}>
      <img src={product.image} alt={product.name} />
      <h3>{product.name}</h3>
      {product.isNew && <Badge tone="success">New</Badge>}
      <p className="price">{product.price.toFixed(2)} €</p>
      <Button onClick={() => onAddToCart(product.id)}>Add to cart</Button>
    </article>
  );
}

export default memo(ProductCard);
`,
  "src/components/ProductGrid.tsx": `import ProductCard from "./ProductCard";
import type { Product } from "@/types";

export interface ProductGridProps {
  products: Product[];
  onAddToCart: (id: string) => void;
}

export const ProductGrid = ({ products, onAddToCart }: ProductGridProps) => (
  <section className="grid">
    {products.map((p) => (
      <ProductCard key={p.id} product={p} onAddToCart={onAddToCart} />
    ))}
  </section>
);
`,
  "src/components/CartDrawer.tsx": `import { Button } from "@/components/ui";
import type { CartItem } from "@/types";

interface CartDrawerProps {
  open: boolean;
  items: CartItem[];
  onClose: () => void;
}

export function CartDrawer({ open, items, onClose }: CartDrawerProps) {
  if (!open) return null;
  const total = items.reduce((sum, i) => sum + i.price * i.quantity, 0);
  return (
    <aside className="drawer">
      <h2>Your cart</h2>
      <ul>
        {items.map((i) => (
          <li key={i.id}>
            {i.quantity} × {i.name}
          </li>
        ))}
      </ul>
      <p className="total">{total.toFixed(2)} €</p>
      <Button variant="primary" size="lg">Checkout</Button>
      <Button variant="ghost" onClick={onClose}>Close</Button>
    </aside>
  );
}
`,
  "src/components/ui/Button.tsx": `import { forwardRef, type ButtonHTMLAttributes } from "react";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "ghost" | "danger";
  size?: "sm" | "md" | "lg";
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ variant = "primary", size = "md", className, ...rest }, ref) => (
    <button ref={ref} className={\`btn btn-\${variant} btn-\${size} \${className ?? ""}\`} {...rest} />
  ),
);
`,
  "src/components/ui/Badge.tsx": `import type { ReactNode } from "react";

export function Badge({ tone = "neutral", children }: { tone?: "neutral" | "accent" | "success"; children: ReactNode }) {
  return <span className={\`badge badge-\${tone}\`}>{children}</span>;
}
`,
  "src/components/ui/index.ts": `export { Button } from "./Button";
export type { ButtonProps } from "./Button";
export * from "./Badge";
`,
  "src/pages/HomePage.tsx": `import { ProductGrid } from "@/components/ProductGrid";
import { useProducts } from "@/hooks/useProducts";

export default function HomePage({ onAddToCart }: { onAddToCart: (id: string) => void }) {
  const { products, loading } = useProducts();
  if (loading) return <p className="muted">Loading products…</p>;
  return <ProductGrid products={products} onAddToCart={onAddToCart} />;
}
`,
  "src/hooks/useCart.ts": `import { useState } from "react";
import type { CartItem } from "@/types";

export function useCart() {
  const [items, setItems] = useState<CartItem[]>([]);
  const [isOpen, setOpen] = useState(false);
  return {
    items,
    isOpen,
    open: () => setOpen(true),
    close: () => setOpen(false),
    add: (id: string) => setItems((list) => [...list, { id, name: id, price: 0, quantity: 1 }]),
  };
}
`,
  "src/hooks/useProducts.ts": `import { useEffect, useState } from "react";
import type { Product } from "@/types";

export function useProducts() {
  const [products, setProducts] = useState<Product[]>([]);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    fetch("/api/products")
      .then((r) => r.json())
      .then(setProducts)
      .finally(() => setLoading(false));
  }, []);
  return { products, loading };
}
`,
  "src/types.ts": `export interface Product {
  id: string;
  name: string;
  price: number;
  image: string;
  isNew?: boolean;
}

export interface CartItem {
  id: string;
  name: string;
  price: number;
  quantity: number;
}
`,
  "src/styles/app.css": `:root {
  --brand: #6d28d9;
  --brand-soft: rgb(109 40 217 / 12%);
  --text-muted: hsl(220, 9%, 46%);
}

.page { max-width: 1200px; margin: 0 auto; }
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 16px; }

.card {
  border-radius: 12px;
  padding: 12px;
  background: white;
  box-shadow: 0 1px 3px rgba(15, 23, 42, 0.12);
}

.badge {
  color: #047857;
  background: rgba(16, 185, 129, 0.15);
}

.price { color: tomato; font-weight: 600; }
`,
};

// ---- Vue: vue-dashboard -----------------------------------------------------------------

const vue: Record<string, string> = {
  "package.json": `{
  "name": "vue-dashboard",
  "version": "0.9.2",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vue-tsc -b && vite build",
    "preview": "vite preview",
    "test:unit": "vitest",
    "lint": "eslint . --fix",
    "format": "prettier --write src/"
  },
  "dependencies": {
    "pinia": "^3.0.2",
    "vue": "^3.5.13",
    "vue-router": "^4.5.1"
  },
  "devDependencies": {
    "@vitejs/plugin-vue": "^5.2.3",
    "typescript": "~5.8.3",
    "vite": "^6.3.5",
    "vitest": "^3.1.2",
    "vue-tsc": "^2.2.10"
  }
}
`,
  "yarn.lock": "# THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.\n# yarn lockfile v1\n",
  "tsconfig.json": `{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "paths": { "@/*": ["./src/*"] }
  },
  "include": ["src/**/*.ts", "src/**/*.vue"]
}
`,
  "vite.config.ts": `import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
});
`,
  "index.html": '<!doctype html>\n<html lang="en">\n  <body>\n    <div id="app"></div>\n    <script type="module" src="/src/main.ts"></script>\n  </body>\n</html>\n',
  ".gitignore": "node_modules\ndist\n",
  "src/main.ts": `import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";

createApp(App).use(createPinia()).mount("#app");
`,
  "src/App.vue": `<script setup lang="ts">
import AppHeader from './components/AppHeader.vue'
import DashboardView from './views/DashboardView.vue'
</script>

<template>
  <AppHeader title="Dashboard" :unread="3" @search="(q) => console.log(q)" />
  <main class="content">
    <DashboardView />
  </main>
</template>

<style scoped>
.content { padding: 24px; }
</style>
`,
  "src/components/AppHeader.vue": `<script setup lang="ts">
import { BaseButton } from '@/components/base'

defineProps<{
  title: string
  unread?: number
}>()

const emit = defineEmits<{
  (e: 'search', query: string): void
  (e: 'logout'): void
}>()
</script>

<template>
  <header class="app-header">
    <h1>{{ title }}</h1>
    <input type="search" placeholder="Search…" @keyup.enter="emit('search', ($event.target as HTMLInputElement).value)" />
    <BaseButton variant="ghost" @click="emit('logout')">Sign out</BaseButton>
  </header>
</template>
`,
  "src/components/StatCard.vue": `<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  label: string
  value: number
  trend?: number
  format?: 'number' | 'currency'
}

const props = withDefaults(defineProps<Props>(), { trend: 0, format: 'number' })

const display = computed(() =>
  props.format === 'currency' ? props.value.toLocaleString('en', { style: 'currency', currency: 'EUR' }) : props.value.toLocaleString('en'),
)
</script>

<template>
  <div class="stat-card">
    <span class="label">{{ label }}</span>
    <strong>{{ display }}</strong>
    <span :class="trend >= 0 ? 'up' : 'down'">{{ trend }}%</span>
  </div>
</template>
`,
  "src/components/OrdersTable.vue": `<script setup lang="ts">
import type { Order } from '@/stores/orders'
import BaseButton from './base/BaseButton.vue'

defineProps<{ orders: Order[]; loading?: boolean }>()
const emit = defineEmits<{ refund: [id: number] }>()
</script>

<template>
  <table class="orders">
    <tr v-for="o in orders" :key="o.id">
      <td>#{{ o.id }}</td>
      <td>{{ o.customer }}</td>
      <td>{{ o.total }} €</td>
      <td><base-button size="sm" variant="danger" @click="emit('refund', o.id)">Refund</base-button></td>
    </tr>
  </table>
</template>
`,
  "src/components/base/BaseButton.vue": `<script setup lang="ts">
withDefaults(
  defineProps<{
    variant?: 'primary' | 'ghost' | 'danger'
    size?: 'sm' | 'md'
    disabled?: boolean
  }>(),
  { variant: 'primary', size: 'md' },
)
defineEmits(['click'])
</script>

<template>
  <button :class="['btn', variant, size]" :disabled="disabled" @click="$emit('click', $event)">
    <slot />
  </button>
</template>
`,
  "src/components/base/index.ts": "export { default as BaseButton } from './BaseButton.vue'\n",
  "src/views/DashboardView.vue": `<script setup lang="ts">
import { onMounted } from 'vue'
import StatCard from '@/components/StatCard.vue'
import OrdersTable from '@/components/OrdersTable.vue'
import { useOrders } from '@/stores/orders'

const store = useOrders()
onMounted(() => store.load())
</script>

<template>
  <section class="stats">
    <StatCard label="Revenue" :value="store.revenue" :trend="12" format="currency" />
    <StatCard label="Orders" :value="store.orders.length" :trend="-3" />
    <stat-card label="Customers" :value="store.customers" />
  </section>
  <OrdersTable :orders="store.orders" :loading="store.loading" @refund="store.refund" />
</template>
`,
  "src/stores/orders.ts": `import { defineStore } from "pinia";

export interface Order {
  id: number;
  customer: string;
  total: number;
}

export const useOrders = defineStore("orders", {
  state: () => ({ orders: [] as Order[], loading: false, customers: 0 }),
  getters: { revenue: (s) => s.orders.reduce((sum, o) => sum + o.total, 0) },
  actions: {
    async load() {
      this.loading = true;
      this.orders = await fetch("/api/orders").then((r) => r.json());
      this.loading = false;
    },
    refund(id: number) {
      this.orders = this.orders.filter((o) => o.id !== id);
    },
  },
});
`,
};

// ---- Angular: admin-portal --------------------------------------------------------------

const angular: Record<string, string> = {
  "package.json": `{
  "name": "admin-portal",
  "version": "2.1.0",
  "scripts": {
    "ng": "ng",
    "start": "ng serve",
    "build": "ng build",
    "watch": "ng build --watch --configuration development",
    "test": "ng test",
    "lint": "ng lint"
  },
  "dependencies": {
    "@angular/common": "^19.2.0",
    "@angular/core": "^19.2.0",
    "@angular/router": "^19.2.0",
    "rxjs": "~7.8.0"
  },
  "devDependencies": {
    "@angular/cli": "^19.2.0",
    "stylelint": "^16.19.1",
    "stylelint-config-standard-scss": "^14.0.0",
    "typescript": "~5.7.2"
  }
}
`,
  "package-lock.json": '{\n  "name": "admin-portal",\n  "lockfileVersion": 3\n}\n',
  "angular.json": '{\n  "version": 1,\n  "projects": { "admin-portal": { "projectType": "application", "root": "", "sourceRoot": "src" } }\n}\n',
  "tsconfig.json": '{\n  "compilerOptions": { "strict": true, "target": "ES2022", "experimentalDecorators": true }\n}\n',
  ".gitignore": "node_modules\ndist\n.angular\n",
  "src/main.ts": `import { bootstrapApplication } from '@angular/platform-browser';
import { AppComponent } from './app/app.component';

bootstrapApplication(AppComponent).catch((err) => console.error(err));
`,
  "src/app/app.component.ts": `import { Component } from '@angular/core';
import { UserListComponent } from './users/user-list.component';

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [UserListComponent],
  templateUrl: './app.component.html',
  styleUrl: './app.component.scss',
})
export class AppComponent {
  title = 'Admin Portal';
}
`,
  "src/app/app.component.html": `<header class="toolbar">
  <h1>{{ title }}</h1>
</header>
<main>
  <app-user-list />
</main>
`,
  "src/app/app.component.scss": ".toolbar { display: flex; padding: 0 16px; }\n",
  "src/app/users/user.ts": "export interface User {\n  id: number;\n  name: string;\n  email: string;\n  createdAt: Date;\n}\n",
  "src/app/users/user-card.component.ts": `import { Component, EventEmitter, Input, Output, input } from '@angular/core';
import { TitleCasePipe } from '@angular/common';
import { HighlightDirective } from '../shared/highlight.directive';
import { RelativeTimePipe } from '../shared/relative-time.pipe';
import { User } from './user';

@Component({
  selector: 'app-user-card',
  standalone: true,
  imports: [TitleCasePipe, HighlightDirective, RelativeTimePipe],
  templateUrl: './user-card.component.html',
  styleUrls: ['./user-card.component.scss', '../shared/card.scss'],
})
export class UserCardComponent {
  @Input({ required: true }) user!: User;
  @Input('compact') isCompact = false;
  size = input<'sm' | 'md'>('md');
  @Output() selected = new EventEmitter<User>();
}
`,
  "src/app/users/user-card.component.html": `<div class="card" [class.compact]="isCompact" appHighlight="#fff3c4" (click)="selected.emit(user)">
  <h3>{{ user.name | titlecase }}</h3>
  <a [href]="'mailto:' + user.email">{{ user.email }}</a>
  <small>Joined {{ user.createdAt | relativeTime }}</small>
</div>
`,
  "src/app/users/user-card.component.scss": `.card {
  border-radius: 8px;
  padding: 12px;
  color: #ffg;
  background: #ffffff;
  margin: 0px;

  &:hover {
    border-color: #333333;
  }
}
`,
  ".stylelintrc.json": '{\n  "extends": ["stylelint-config-standard-scss"]\n}\n',
  "src/app/users/user-list.component.ts": `import { Component, output } from '@angular/core';
import { UserCardComponent } from './user-card.component';
import { User } from './user';

@Component({
  selector: 'app-user-list',
  standalone: true,
  imports: [UserCardComponent],
  template: \`
    <section class="users">
      @for (u of users; track u.id) {
        <app-user-card [user]="u" compact (selected)="picked.emit($event)" />
      }
    </section>
  \`,
  styleUrl: './user-list.component.scss',
})
export class UserListComponent {
  users: User[] = [];
  picked = output<User>();
}
`,
  "src/app/users/user-list.component.scss": ".users { display: grid; gap: 12px; }\n",
  "src/app/shared/card.scss": ".card { box-shadow: 0 1px 3px rgb(0 0 0 / 12%); }\n",
  "src/app/shared/highlight.directive.ts": `import { Directive, ElementRef, HostListener, Input, inject } from '@angular/core';

@Directive({ selector: '[appHighlight]', standalone: true })
export class HighlightDirective {
  @Input() appHighlight = '';
  private el = inject(ElementRef);

  @HostListener('mouseenter') onEnter() {
    this.el.nativeElement.style.backgroundColor = this.appHighlight || 'yellow';
  }

  @HostListener('mouseleave') onLeave() {
    this.el.nativeElement.style.backgroundColor = '';
  }
}
`,
  "src/app/shared/relative-time.pipe.ts": `import { Pipe, PipeTransform } from '@angular/core';

@Pipe({ name: 'relativeTime', standalone: true })
export class RelativeTimePipe implements PipeTransform {
  transform(value: Date): string {
    const days = Math.round((Date.now() - value.getTime()) / 86_400_000);
    return days === 0 ? 'today' : \`\${days} days ago\`;
  }
}
`,
};


// ---- Node.js: orders-api --------------------------------------------------------------

/** `src/orders/pricing.ts`, where the JavaScript debugger demo stops (line 12). */
export const NODE_PRICING = `import type { LineItem, Order } from "../types";

export const TAX_RATE = 0.19;

export function lineTotal(item: LineItem): number {
  return item.unitPrice * item.quantity;
}

export function orderTotal(order: Order): number {
  const subtotal = order.items.reduce((sum, item) => sum + lineTotal(item), 0);
  const discount = order.voucher ? (subtotal * order.voucher.percent) / 100 : 0;
  const tax = (subtotal - discount) * TAX_RATE;
  return Math.round((subtotal - discount + tax) * 100) / 100;
}
`;

const node: Record<string, string> = {
  "package.json": `{
  "name": "orders-api",
  "version": "2.3.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "tsx watch src/server.ts",
    "build": "tsc -p .",
    "start": "node dist/server.js",
    "test": "vitest run",
    "test:e2e": "playwright test"
  },
  "devDependencies": {
    "@playwright/test": "^1.49.1",
    "@biomejs/biome": "1.9.4",
    "@types/node": "^22.10.2",
    "tsx": "^4.19.2",
    "typescript": "~5.7.2",
    "vitest": "^2.1.8"
  }
}
`,
  "package-lock.json": '{\n  "name": "orders-api",\n  "lockfileVersion": 3,\n  "requires": true\n}\n',
  "tsconfig.json": `{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "outDir": "dist",
    "sourceMap": true,
    "strict": true
  },
  "include": ["src"]
}
`,
  "vitest.config.ts": `import { defineConfig } from "vitest/config";

export default defineConfig({
  test: { include: ["src/**/*.test.ts"], coverage: { provider: "v8" } },
});
`,
  "playwright.config.ts": `import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "e2e",
  use: { baseURL: "http://localhost:3000" },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: { command: "npm run dev", url: "http://localhost:3000/health" },
});
`,
  ".gitignore": "node_modules\ndist\ntest-results\nplaywright-report\n",
  "src/types.ts": `export interface LineItem {
  sku: string;
  name: string;
  unitPrice: number;
  quantity: number;
}

export interface Order {
  id: number;
  items: LineItem[];
  voucher?: { code: string; percent: number };
}
`,
  "src/orders/pricing.ts": NODE_PRICING,
  "biome.json": `{
  "$schema": "https://biomejs.dev/schemas/1.9.4/schema.json",
  "linter": { "enabled": true, "rules": { "recommended": true } },
  "formatter": { "enabled": true, "indentStyle": "space" }
}
`,
  "src/orders/vouchers.ts": `import type { Order } from "../types";

export function voucherPercent(order: Order): number {
  let percent = order.voucher?.percent ?? 0;
  if (percent == 0) return 0;
  return Math.min(percent, 100);
}

export function voucherCode(order: Order): string {
  return order.voucher!.code.toUpperCase();
}
`,
  "src/orders/pricing.test.ts": `import { describe, expect, it } from "vitest";
import { lineTotal, orderTotal } from "./pricing";

const keyboard = { sku: "KB-1", name: "Keyboard", unitPrice: 49.99, quantity: 1 };
const cable = { sku: "USB-C", name: "USB-C cable", unitPrice: 4.99, quantity: 2 };

describe("pricing", () => {
  it("multiplies the unit price", () => {
    expect(lineTotal(cable)).toBeCloseTo(9.98);
  });

  describe("orderTotal", () => {
    it("adds the tax", () => {
      expect(orderTotal({ id: 1, items: [keyboard] })).toBe(59.49);
    });

    it("applies the voucher before the tax", () => {
      const order = { id: 2, items: [keyboard, cable], voucher: { code: "SPRING10", percent: 10 } };
      expect(orderTotal(order)).toBe(64.24);
    });

    it("rounds to cents", () => {
      expect(orderTotal({ id: 3, items: [cable] })).toBe(11.88);
    });
  });
});
`,
  "src/orders/store.ts": `import type { Order } from "../types";

const orders = new Map<number, Order>([
  [1042, {
    id: 1042,
    items: [
      { sku: "KB-1", name: "Keyboard", unitPrice: 49.99, quantity: 1 },
      { sku: "USB-C", name: "USB-C cable", unitPrice: 4.99, quantity: 2 },
    ],
    voucher: { code: "SPRING10", percent: 10 },
  }],
]);

export async function findOrder(id: number): Promise<Order | undefined> {
  return orders.get(id);
}
`,
  "src/orders/store.test.ts": `import { describe, expect, it } from "vitest";
import { findOrder } from "./store";

describe("store", () => {
  it("finds an order by id", async () => {
    expect((await findOrder(1042))?.items).toHaveLength(2);
  });

  it("returns undefined for unknown ids", async () => {
    expect(await findOrder(1)).toBeUndefined();
  });
});
`,
  "src/server.ts": `import { createServer } from "node:http";
import { orderTotal } from "./orders/pricing";
import { findOrder } from "./orders/store";

const PORT = Number(process.env.PORT ?? 3000);

const server = createServer(async (req, res) => {
  const url = new URL(req.url ?? "/", \`http://localhost:\${PORT}\`);
  const match = /^\\/orders\\/(\\d+)\\/total$/.exec(url.pathname);
  if (!match) {
    res.writeHead(404).end();
    return;
  }
  const order = await findOrder(Number(match[1]));
  if (!order) {
    res.writeHead(404, { "content-type": "application/json" }).end(JSON.stringify({ error: "order not found" }));
    return;
  }
  const total = orderTotal(order);
  res.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify({ id: order.id, total }));
});

server.listen(PORT, () => console.log(\`Orders API listening on http://localhost:\${PORT}\`));
`,
  "e2e/checkout.spec.ts": `import { expect, test } from "@playwright/test";

test("reports the health", async ({ request }) => {
  const res = await request.get("/health");
  expect(res.ok()).toBeTruthy();
});

test.describe("order totals", () => {
  test("returns the total of an order", async ({ request }) => {
    const res = await request.get("/orders/1042/total");
    expect(await res.json()).toEqual({ id: 1042, total: 64.24 });
  });

  test("answers 404 for unknown orders", async ({ request }) => {
    const res = await request.get("/orders/1/total");
    expect(res.status()).toBe(404);
  });
});
`,
  "README.md": "# orders-api\n\nOrder totals over HTTP. `npm run dev` starts the server on port 3000, `npm test` runs the Vitest unit tests and `npm run test:e2e` the Playwright tests.\n",
};

const base = { packs: ["webforge"], languages: ["javascript", "typescript", "html", "css"] };

export const webDemos: Record<WebDemoId, WebDemo> = {
  react: {
    name: "storefront-ui",
    initialFile: "src/App.tsx",
    files: react,
    info: {
      ...base,
      frameworks: [fw("react", "React", "19.1.0"), fw("vite", "Vite", "6.3.5")],
      tools: ["git", "pnpm"],
      primary: fw("react", "React", "19.1.0"),
    },
  },
  vue: {
    name: "vue-dashboard",
    initialFile: "src/views/DashboardView.vue",
    files: vue,
    info: {
      ...base,
      frameworks: [fw("vue", "Vue.js", "3.5.13"), fw("vite", "Vite", "6.3.5")],
      tools: ["git", "yarn"],
      primary: fw("vue", "Vue.js", "3.5.13"),
    },
  },
  node: {
    name: "orders-api",
    initialFile: "src/orders/pricing.ts",
    files: node,
    info: {
      ...base,
      frameworks: [fw("node", "Node.js", "22.11.0", "typescript")],
      tools: ["git", "npm"],
      primary: fw("node", "Node.js", "22.11.0", "typescript"),
    },
  },
  angular: {
    name: "admin-portal",
    initialFile: "src/app/users/user-card.component.ts",
    files: angular,
    info: {
      ...base,
      frameworks: [fw("angular", "Angular", "19.2.0", "typescript")],
      tools: ["git", "npm"],
      primary: fw("angular", "Angular", "19.2.0", "typescript"),
    },
  },
};

/** The open WebForge demo, if the page asks for one. */
export const webDemo = (): WebDemo | null => {
  const id = webDemoId();
  return id ? webDemos[id] : null;
};
