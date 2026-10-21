/**
 * Documentation, data and image files of the demo project, for the Markdown preview, the
 * image editor and "Edit as Table" in browser mode and screenshots.
 */

/** The PNG files of the demo project (base64); `mockFiles` lists them with empty text. */
export const demoImages: Record<string, string> = {
  "public/images/logo.png": "iVBORw0KGgoAAAANSUhEUgAAAIAAAACACAYAAADDPmHLAAABq0lEQVR42u3awY3CMBRF0RRCBxELGqEMVlRJB3RAJ2HPDrAd/7zzpN+A71GkQbMsZmZmZmZmZmaD9rqvW/IJ7Y4LQ8hQDKKFQhApGIIwwQgECUYgRDACAYIRePhgBB683W3PWy0EorUL/3klEIjXJ34rBOKHA+iKQLy+8af+CohX5wvQBYFwAIhX4K+ALgAEq/E7QDcEQoX/OugxAfCgAPx36/nifjwAANgPQKvPkZDjATRBAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAED/kAAAAAAAAAAAAAAAAFAPwKhLCQsAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAN/ucT1tlW4PADO/BwAA1AZwxH8PBwAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABwADgAHgAPAAeAAcAA4ANxAABCExwcAAAAA8JgAOABcKAAIwuMDAAAA6QAgCI8PAAAQpMcHAAAI0uNDID4E4kMgPgTiQyA+BOKDIDwIwsMgOhhCm5mZmZmZmdnEewMZlSiDuqIP3AAAAABJRU5ErkJggg==",
};

const readme = `---
title: Acme Shop
owner: platform-team
tags: [laravel, shop]
---
# Acme Shop

![Acme Shop logo](public/images/logo.png)

A demo **Laravel 12** shop for Forge. See the [architecture notes](docs/architecture.md#order-flow)
and the [order model](app/Models/Order.php). Issues go to https://github.com/acme/shop.

> [!NOTE]
> Run \`php artisan migrate --seed\` before the first start.

## Features

| Area | Status | Owner |
|:-----|:------:|------:|
| Checkout | ~~beta~~ stable | @mia |
| Invoices | planned | @tom |
| Search | in progress | @ana |

- [x] Orders and order items
- [x] Stripe checkout
- [ ] Multi-currency prices[^fx]

## Order flow

\`\`\`mermaid
flowchart LR
    Cart --> Checkout
    Checkout -->|paid| Order
    Order --> Invoice
    Checkout -->|failed| Cart
\`\`\`

\`\`\`php
\$order = Order::query()->with('items')->findOrFail(\$id);
\$order->markAsPaid();
\`\`\`

## Data

Exported orders live in [storage/app/orders.csv](storage/app/orders.csv).

[^fx]: Exchange rates come from the ECB daily feed.
`;

const architecture = `# Architecture

## Order flow

Orders move from the cart to checkout; a paid checkout creates an order and its invoice.

## Services

- \`App\\Services\\Checkout\` talks to Stripe.
- \`App\\Services\\Invoices\` renders PDFs.
`;

export const demoDocFiles: Record<string, string> = {
  "README.md": readme,
  "docs/architecture.md": architecture,
  "storage/app/orders.csv": `id,customer,total,currency,status,created_at
1001,"Mia Svensson",129.90,EUR,paid,2026-09-01
1002,Tom Becker,49.00,EUR,refunded,2026-09-02
1003,"Ana López, GmbH",1240.50,EUR,paid,2026-09-03
1004,Ken Ito,18.75,USD,open,2026-09-05
1005,Lea Martin,310.00,EUR,paid,2026-09-08
1006,"Sam ""Chef"" Rossi",72.40,EUR,open,2026-09-09
`,
  "public/images/logo.png": "",
  "public/images/hero.svg": `<svg xmlns="http://www.w3.org/2000/svg" width="240" height="120" viewBox="0 0 240 120">
  <rect width="240" height="120" rx="14" fill="#2b2d31"/>
  <circle cx="60" cy="60" r="34" fill="#e06c2b"/>
  <path d="M42 52h40v8H70v14h10v8H44v-8h10V60H42z" fill="#2b2d31"/>
  <text x="110" y="56" font-family="sans-serif" font-size="22" fill="#f2f2f2">Acme</text>
  <text x="110" y="82" font-family="sans-serif" font-size="16" fill="#ffd166">Shop</text>
</svg>
`,
};
