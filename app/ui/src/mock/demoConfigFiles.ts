/** Non-PHP files of the demo project, so every syntax mode has something to show. */
export const demoConfigFiles: Record<string, string> = {
  ".env": `# Application
APP_NAME="Acme Shop"
APP_ENV=local
APP_KEY=base64:3q2+7w==
APP_DEBUG=true
APP_URL=http://localhost:8000

# Database
DB_CONNECTION=mysql
DB_HOST=127.0.0.1
DB_PORT=3306
DB_DATABASE=shop
DB_USERNAME=shop
DB_PASSWORD=secret

MAIL_MAILER=smtp
MAIL_FROM_ADDRESS="hello@acme.test"
`,
  "docker-compose.yml": `services:
  app:
    build:
      context: .
      dockerfile: Dockerfile
    ports:
      - "8000:8000"
    environment:
      DB_HOST: mysql
      XDEBUG_MODE: debug
    depends_on:
      - mysql

  mysql:
    image: mysql:8.4
    environment:
      MYSQL_DATABASE: shop
      MYSQL_ROOT_PASSWORD: secret # local only
    volumes:
      - db-data:/var/lib/mysql

volumes:
  db-data: {}
`,
  Dockerfile: `FROM php:8.3-fpm-alpine

RUN apk add --no-cache git unzip \\
    && docker-php-ext-install pdo_mysql opcache

COPY --from=composer:2 /usr/bin/composer /usr/bin/composer
WORKDIR /var/www/html
COPY . .
RUN composer install --no-dev --optimize-autoloader

EXPOSE 8000
CMD ["php", "artisan", "serve", "--host=0.0.0.0"]
`,
  "database/schema.sql": `-- Reporting view for paid orders
CREATE TABLE IF NOT EXISTS products (
    id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    sku VARCHAR(32) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    price DECIMAL(10, 2) NOT NULL DEFAULT 0.00,
    created_at TIMESTAMP NULL
);

SELECT u.name, COUNT(o.id) AS orders, SUM(o.total) AS revenue
FROM users u
    JOIN orders o ON o.user_id = u.id
WHERE o.status = 'paid'
  AND o.created_at >= NOW() - INTERVAL 30 DAY
GROUP BY u.id
ORDER BY revenue DESC
LIMIT 10;
`,
  "package.json": `{
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build"
  },
  "devDependencies": {
    "laravel-vite-plugin": "^1.2.0",
    "tailwindcss": "^4.0.0",
    "vite": "^7.0.0"
  }
}
`,
  "phpunit.xml": `<?xml version="1.0" encoding="UTF-8"?>
<phpunit xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         bootstrap="vendor/autoload.php"
         colors="true">
    <testsuites>
        <testsuite name="Feature">
            <directory>tests/Feature</directory>
        </testsuite>
    </testsuites>
    <php>
        <env name="APP_ENV" value="testing"/>
        <env name="DB_CONNECTION" value="sqlite"/>
    </php>
</phpunit>
`,
  "resources/js/app.ts": `import axios from "axios";

interface Order {
  id: number;
  total: number;
  status: "pending" | "paid" | "refunded";
}

export async function loadOrders(userId: number): Promise<Order[]> {
  const { data } = await axios.get<Order[]>(\`/api/users/\${userId}/orders\`);
  return data.filter((o) => o.status !== "refunded");
}
`,
  "resources/css/app.css": `@import "tailwindcss";

:root {
  --brand: #ff7a1a;
}

.btn-primary {
  background: var(--brand);
  border-radius: 6px;
  padding: 0.5rem 1rem;
}
`,
  ".github/workflows/tests.yml": `name: tests
on: [workflow_dispatch]
jobs:
  phpunit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: composer install && php artisan test
`,
  "templates/order/show.html.twig": `{# Order detail page #}
{% extends 'base.html.twig' %}

{% block title %}Order #{{ order.id }}{% endblock %}

{% block body %}
    <section class="order" data-status="{{ order.status|lower }}">
        <h1>{{ 'order.title'|trans({'%id%': order.id}) }}</h1>
        {% if order.items is empty %}
            <p class="muted">No items yet.</p>
        {% else %}
            <ul>
                {% for item in order.items %}
                    <li>{{ item.name }} &times; {{ item.quantity }} — {{ item.total|format_currency('EUR') }}</li>
                {% endfor %}
            </ul>
        {% endif %}
        <a href="{{ path('orders.index') }}">Back</a>
    </section>
{% endblock %}
`,
  "legacy/templates/cart.tpl": `{* Legacy shop cart (Smarty) *}
{extends file="layout.tpl"}

{block name=content}
<div class="cart">
  <h2>{$title|escape}</h2>
  {if $items|@count gt 0}
    <table>
      {foreach from=$items item=item name=cart}
        <tr class="{cycle values='odd,even'}">
          <td>{$item.name}</td>
          <td>{$item.price|string_format:"%.2f"} &euro;</td>
        </tr>
      {/foreach}
    </table>
  {else}
    <p>Your cart is empty.</p>
  {/if}
</div>
{/block}
`,
  "public/index.html": `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Acme Shop</title>
    <link rel="stylesheet" href="/build/app.css" />
  </head>
  <body class="antialiased">
    <!-- Mounted by resources/js/app.ts -->
    <div id="app" data-user-id="42"></div>
    <script type="module" src="/build/app.js"></script>
  </body>
</html>
`,
};
