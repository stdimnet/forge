/**
 * The web tooling demo (`?project=webtools`): an Nx workspace with a Nuxt storefront, an
 * API described by OpenAPI, a GraphQL schema with queries, a Prisma schema, Sass styles
 * compiled by a File Watcher, Vite aliases and `.http` requests (GraphQL, WebSocket, gRPC,
 * OAuth 2.0).
 */
export const webToolingFiles: Record<string, string> = {
  "package.json": `{
  "name": "@acme/shop-platform",
  "private": true,
  "packageManager": "pnpm@9.15.4",
  "workspaces": ["apps/*", "libs/*"],
  "scripts": {
    "dev": "nx run storefront:serve",
    "build": "nx run-many -t build",
    "db:generate": "prisma generate"
  },
  "devDependencies": {
    "nx": "^20.4.0",
    "prisma": "^6.3.0",
    "sass": "^1.85.0",
    "typescript": "~5.8.3",
    "vite": "^6.3.5"
  },
  "dependencies": {
    "@prisma/client": "^6.3.0",
    "graphql": "^16.10.0"
  }
}
`,
  "nx.json": `{
  "$schema": "./node_modules/nx/schemas/nx-schema.json",
  "defaultBase": "main",
  "targetDefaults": {
    "build": { "dependsOn": ["^build"], "cache": true },
    "test": { "cache": true },
    "lint": { "cache": true }
  }
}
`,
  "bunfig.toml": `[install]
exact = true
`,
  "vite.config.ts": `import { defineConfig } from "vite";
import { fileURLToPath, URL } from "node:url";
import path from "node:path";

export default defineConfig({
  resolve: {
    alias: {
      "@ui": path.resolve(__dirname, "libs/ui/src"),
      "@styles": fileURLToPath(new URL("./assets/scss", import.meta.url)),
    },
  },
});
`,
  "apps/storefront/project.json": `{
  "name": "storefront",
  "projectType": "application",
  "tags": ["scope:shop", "type:app"],
  "implicitDependencies": ["ui"],
  "targets": {
    "serve": { "executor": "@nx/vite:dev-server" },
    "build": { "executor": "@nx/vite:build" },
    "test": { "executor": "@nx/vite:test" },
    "lint": { "command": "eslint apps/storefront" }
  }
}
`,
  "apps/storefront/nuxt.config.ts": `export default defineNuxtConfig({
  css: ["~/assets/main.css"],
  devtools: { enabled: true },
});
`,
  "apps/storefront/app.vue": `<template>
  <NuxtLayout>
    <AppHeader />
    <NuxtPage />
  </NuxtLayout>
</template>
`,
  "apps/storefront/pages/index.vue": `<script setup lang="ts">
import { formatPrice } from "@ui/format";

const { data: products } = await useFetch("/api/products");
</script>

<template>
  <section class="grid">
    <ProductCard v-for="p in products" :key="p.id" :product="p" />
    <base-button @click="refresh">Load more</base-button>
    <LazyNewsletterForm />
  </section>
</template>
`,
  "apps/storefront/pages/products/[id].vue": `<script setup lang="ts">
const route = useRoute();
const { data: product } = await useFetch(\`/api/products/\${route.params.id}\`);
</script>

<template>
  <ProductCard :product="product" detailed />
</template>
`,
  "apps/storefront/pages/account/orders.vue": "<template><h1>Orders</h1></template>\n",
  "apps/storefront/pages/docs/[...slug].vue": "<template><ContentDoc /></template>\n",
  "apps/storefront/components/AppHeader.vue": "<template><header class=\"header\">ACME Shop</header></template>\n",
  "apps/storefront/components/ProductCard.vue": `<script setup lang="ts">
defineProps<{ product: { id: number; name: string; price: number }; detailed?: boolean }>();
</script>

<template>
  <article class="card">{{ product.name }}</article>
</template>
`,
  "apps/storefront/components/base/Button.vue": "<template><button class=\"btn\"><slot /></button></template>\n",
  "apps/storefront/components/NewsletterForm.vue": "<template><form><input type=\"email\" /></form></template>\n",
  "apps/api/project.json": `{
  "name": "api",
  "projectType": "application",
  "tags": ["scope:api"],
  "targets": {
    "serve": { "command": "bun run apps/api/src/main.ts" },
    "test": { "command": "bun test apps/api" }
  }
}
`,
  "apps/api/openapi.yaml": `openapi: 3.0.3
info:
  title: ACME Shop API
  version: 2.4.0
  description: Products, carts and orders of the ACME shop.
servers:
  - url: https://api.acme.test/v2
    description: Staging
tags:
  - name: products
    description: The catalog
  - name: orders
    description: Checkout and order history
paths:
  /products:
    get:
      tags: [products]
      summary: List products
      operationId: listProducts
      parameters:
        - name: page
          in: query
          required: true
          schema: { type: integer, minimum: 1 }
        - name: category
          in: query
          schema: { type: string }
      responses:
        '200':
          description: A page of products
          content:
            application/json:
              schema:
                type: array
                items: { $ref: '#/components/schemas/Product' }
    post:
      tags: [products]
      summary: Create a product
      operationId: createProduct
      requestBody:
        required: true
        content:
          application/json:
            schema: { $ref: '#/components/schemas/Product' }
      responses:
        '201': { description: Created }
        '422': { description: Validation failed }
  /products/{id}:
    parameters:
      - name: id
        in: path
        required: true
        schema: { type: integer }
    get:
      tags: [products]
      summary: Get a product
      operationId: getProduct
      responses:
        '200':
          description: The product
          content:
            application/json:
              schema: { $ref: '#/components/schemas/Product' }
        '404': { description: Not found }
  /orders:
    post:
      tags: [orders]
      summary: Place an order
      operationId: placeOrder
      security:
        - oauth: [orders:write]
      requestBody:
        content:
          application/json:
            schema: { $ref: '#/components/schemas/Order' }
      responses:
        '201': { description: The new order }
components:
  schemas:
    Product:
      type: object
      required: [name, price]
      properties:
        id: { type: integer, readOnly: true }
        name: { type: string, example: Espresso Cup }
        price: { type: number, format: double, example: 12.5 }
        tags: { type: array, items: { type: string } }
    Order:
      type: object
      required: [items]
      properties:
        items:
          type: array
          items:
            type: object
            properties:
              productId: { type: integer, example: 42 }
              quantity: { type: integer, example: 2 }
        placedAt: { type: string, format: date-time }
`,
  "apps/api/src/main.ts": `Bun.serve({ port: 3000, fetch: () => new Response("ok") });
`,
  "libs/ui/package.json": `{ "name": "@acme/ui", "version": "0.3.0", "scripts": { "test": "vitest run" } }
`,
  "libs/ui/src/format.ts": `export const formatPrice = (cents: number) => \`\${(cents / 100).toFixed(2)} €\`;
`,
  "libs/ui/src/index.ts": `export * from "./format";
`,
  "assets/scss/app.scss": `@use "variables";

.header {
  background: variables.$brand;
  padding: variables.$gap * 2;
}

.card {
  border: 1px solid variables.$border;
}
`,
  "assets/scss/_variables.scss": `$brand: #e2591b;
$border: #d6d2cc;
$gap: 8px;
`,
  "assets/scss/print.scss": `@use "variables";

body { color: black; }
`,
  "prisma/schema.prisma": `// The shop's database.
generator client {
  provider = "prisma-client-js"
}

datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

model Product {
  id        Int         @id @default(autoincrement())
  name      String
  price     Decimal     @db.Decimal(10, 2)
  tags      String[]
  items     OrderItem[]
  createdAt DateTime    @default(now())
}

model Order {
  id       Int         @id @default(autoincrement())
  status   OrderStatus @default(OPEN)
  items    OrderItem[]
  placedAt DateTime?
}

model OrderItem {
  id        Int     @id @default(autoincrement())
  order     Order   @relation(fields: [orderId], references: [id])
  orderId   Int
  product   Product @relation(fields: [productId], references: [id])
  productId Int
  quantity  Int     @default(1)

  @@unique([orderId, productId])
}

enum OrderStatus {
  OPEN
  PAID
  SHIPPED
}
`,
  ".graphqlrc.yml": `schema: graphql/schema.graphql
documents: 'graphql/**/*.graphql'
extensions:
  endpoints:
    staging:
      url: https://api.acme.test/graphql
      headers:
        Authorization: Bearer \${GRAPHQL_TOKEN}
`,
  "graphql/schema.graphql": `"""A product of the catalog."""
type Product {
  id: ID!
  name: String!
  "Price in cents"
  price: Int!
  tags: [String!]!
  reviews(first: Int = 5): [Review!]!
  sku: String @deprecated(reason: "Use id")
}

type Review {
  id: ID!
  rating: Int!
  text: String
}

type Order {
  id: ID!
  status: OrderStatus!
  total: Int!
}

enum OrderStatus {
  OPEN
  PAID
  SHIPPED
}

type Query {
  products(category: String, first: Int = 20): [Product!]!
  product(id: ID!): Product
  order(id: ID!): Order
}

type Mutation {
  placeOrder(productIds: [ID!]!): Order!
}
`,
  "graphql/products.graphql": `query ProductPage($id: ID!) {
  product(id: $id) {
    id
    name
    price
    colour
    reviews(first: 3) {
      rating
      text
    }
  }
}

mutation Checkout {
  placeOrder(productIds: ["42"]) {
    id
    status
  }
}
`,
  "requests/shop.http": `### List products (OAuth 2.0 client credentials)
GET https://api.acme.test/v2/products?page=1
Authorization: Bearer {{$auth.token("shop-api")}}

### GraphQL
GRAPHQL https://api.acme.test/graphql

query ProductPage($id: ID!) {
  product(id: $id) { name price }
}

{
  "id": "42"
}

### Live order updates
WEBSOCKET ws://localhost:8080/orders/live

{"subscribe": "orders"}
=== wait-for-server
{"ping": 1}

### gRPC (server reflection)
GRPC localhost:50051/acme.inventory.v1.Stock/GetLevel

{
  "productId": 42
}
`,
  "http-client.env.json": `{
  "dev": {
    "host": "api.acme.test",
    "Security": {
      "Auth": {
        "shop-api": {
          "Type": "OAuth2",
          "Grant Type": "Client Credentials",
          "Token URL": "https://id.acme.test/oauth/token",
          "Client ID": "storefront",
          "Scope": "products:read orders:write"
        }
      }
    }
  }
}
`,
  "http-client.private.env.json": `{
  "dev": {
    "Security": { "Auth": { "shop-api": { "Client Secret": "local-dev-secret" } } }
  }
}
`,
  ".gitignore": "node_modules\ndist\n.nx\n",
  "README.md": "# ACME shop platform\n\nNx workspace: the Nuxt storefront (`apps/storefront`), the Bun API (`apps/api`) and the UI library (`libs/ui`).\n",
};

/** The File Watcher the demo has configured (Settings | Tools | File Watchers). */
export const webToolingWatchers = [
  {
    name: "SCSS",
    enabled: "true",
    template: "scss",
    fileType: "scss",
    scope: "Project Files",
    program: "sass",
    arguments: "$FileName$:$FileNameWithoutExtension$.css",
    outputPaths: "$FileNameWithoutExtension$.css:$FileNameWithoutExtension$.css.map",
    workingDirectory: "$FileDir$",
    environment: "",
    autoSave: "false",
    onExternal: "false",
    trackOnlyRoot: "true",
    showConsole: "error",
    outputFilters: "$FILE_PATH$ $LINE$:$COLUMN$",
  },
  {
    name: "TypeScript",
    enabled: "false",
    template: "typescript",
    fileType: "typescript",
    scope: "Project Files",
    program: "tsc",
    arguments: "--sourceMap $FileName$",
    outputPaths: "$FileNameWithoutExtension$.js:$FileNameWithoutExtension$.js.map",
    workingDirectory: "$FileDir$",
    environment: "",
    autoSave: "false",
    onExternal: "false",
    trackOnlyRoot: "false",
    showConsole: "error",
    outputFilters: "$FILE_PATH$\\($LINE$,$COLUMN$\\)",
  },
];
