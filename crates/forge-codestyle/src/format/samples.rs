//! The sample code of each Code Style page. The samples are deliberately a little untidy so
//! every option visibly changes the preview.

const PHP: &str = r#"<?php
namespace App\Http\Controllers;
use App\Models\Order;
use Illuminate\Http\Request;
/**
* Lists and shows orders.
*/
#[Controller]
class OrderController extends Controller implements HasMiddleware {
    private const STATUSES = array('new', 'paid', 'shipped');
    public function __construct(private readonly OrderRepository $orders) {}
    public function index(Request $request, int $page = 1): ?array {
        $total = 0;
        foreach ($this->orders->all() as $order) {
            if (!$order->isPaid() && $order->total > 100) {
                $total += $order->total * 2;
            } elseif ($order->total === NULL) {
                continue;
            } else {
                $total -= (int)$order->discount;
            }
        }
        $label = 'Total: ' . $total;
        $items = array_map(function ($item) use ($label) {
            return $label . $item;
        }, ['a', 'b']);
        try {
            $this->log($label ?? 'none');
        } catch (\Exception $e) {
            report($e);
        }
        return [
            'total' => $total,
            'items' => $items
        ];
    }
    public function show(int $id): string {
        $status = match($id) { 1 => 'new', default => 'other' };
        return $status;
    }
}
"#;

const JS: &str = r#"import { readFile } from "fs";
const DEFAULTS = { retries: 3, delay: 100 };
export class Queue extends EventEmitter {
    constructor(options) {
        super();
        this.options = { ...DEFAULTS, ...options };
        this.items = [];
    }
    async run(task, attempt = 0) {
        for (let i = 0; i < this.items.length; i++) {
            if (this.items[i] === task && attempt <= this.options.retries) {
                await this.process(this.items[i]);
            } else {
                this.emit('skip', i * 2 + 1);
            }
        }


        const total = this.items.reduce(function (sum, item) {
            return sum + item.size;
        }, 0);
        return total > 0 ? total : null;
    }
}
export function sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
}
"#;

const TS: &str = r#"import { readFile } from "fs";
interface Options { retries: number; delay?: number }
export class Queue<T> {
    private items: T[] = [];
    constructor(private readonly options: Options) {
        this.items = [];
    }
    async run(task: T, attempt: number = 0): Promise<number | null> {
        for (let i = 0; i < this.items.length; i++) {
            if (this.items[i] === task && attempt <= this.options.retries) {
                await this.process(this.items[i]);
            } else {
                this.emit('skip', i * 2 + 1);
            }
        }


        const total = this.items.reduce(function (sum: number, item: T) {
            return sum + 1;
        }, 0);
        return total > 0 ? total : null;
    }
}
export function sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
}
"#;

const CSS: &str = r#"$primary: #3A7BD5;
@import 'base';
.card, .panel > .header {
    color: $primary;
    background: #FFF url(img/bg.png) no-repeat;
    margin: 0 auto;
    &:hover { color: darken($primary, 10%); }
}
@media (max-width: 600px) {
    .card { padding: 8px 16px; }
}
"#;

const JSON: &str = r#"{"name": "forge/demo", "version": "1.0.0",
  "require": {"php": "^8.2", "laravel/framework": "^11.0"},
  "keywords": ["ide", "php"], "private": true, "config": {}}
"#;

const YAML: &str = r#"# Docker Compose
services:
  app:
    image: "forge/app:latest"
    ports:
    - "8080:80"
    - "9000:9000"
    environment: {APP_ENV: local, DEBUG: true}
    command: [php, artisan, serve]



  db:
    image: 'mysql:8'
    volumes:
      - data:/var/lib/mysql
    healthcheck:
      test: >
        mysqladmin ping
        -h localhost
volumes:
  data: {}
"#;

const LUA: &str = r#"local Queue = {}
Queue.__index = Queue
function Queue.new(options)
    local self = setmetatable({}, Queue)
    self.items = {retries = options.retries or 3, 'first'}
    return self
end
function Queue:push(item)
    if item == nil then
        return false
    end
    table.insert(self.items, item)
    print('pushed ' .. #self.items .. " items")
    return true
end
local util = require "queue.util"
return Queue
"#;

const SQL: &str = r#"select o.id, o.total, c.name as customer
from orders o
inner join customers c on c.id = o.customer_id
where o.total>100 and o.status in ('paid','shipped')
order by o.created_at desc
limit 10;
insert into audit(order_id,created_at) values (1,now());
"#;

const HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<title>Orders</title>
<style>
body { margin: 0; }
</style>
</head>
<body>
<nav class="menu" id="main" data-open="false">
<ul>
<li><a href="/orders">Orders</a></li>
<li><a href = "/customers">Customers</a></li>
</ul>
</nav>


<img src="logo.png" alt="Logo"/>
<pre>
  keep   this
</pre>
</body>
</html>
"#;

const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project name="forge" default="build">
<property name="src" value="src"/>
<target name="build" depends="test">
<echo message="Building"/>
<!-- compile -->
<exec executable = "php">
<arg value="artisan"/>
</exec>
</target>
</project>
"#;

const BLADE: &str = r#"@extends('layouts.app')
@section('content')
<div class="orders">
@if($orders->isEmpty())
<p>{{$message}}</p>
@else
<ul>
@foreach ($orders as $order)
<li>{{ $order->number }} – {!!$order->total!!}</li>
@endforeach
</ul>
@endif
</div>
@endsection
"#;

const TWIG: &str = r#"{% extends 'base.html.twig' %}
{% block body %}
<ul class="orders">
{% for order in orders %}
<li>{{order.number}} – {{ order.total|number_format(2) }}</li>
{% else %}
<li>{{'No orders'|trans}}</li>
{% endfor %}
</ul>
{% endblock %}
"#;

const MARKDOWN: &str = r#"#Orders
Forge formats Markdown too.
-  first item
*   second item



>quoted text
```php
$total = 0;
```
## Totals
| Status | Count |
|--------|-------|
| paid   | 12    |
"#;

const GENERAL: &str = r#"<?php
$matrix = [
    1,0,0,
    0,1,0,
];
// @formatter:off
$identity = [
    1,0,0,
    0,1,0,
];
// @formatter:on
$sum = array_sum($matrix)+array_sum($identity);
"#;

/// The sample of a Code Style page.
pub fn sample(language: &str) -> Option<&'static str> {
    Some(match language {
        "php" => PHP,
        "javascript" => JS,
        "typescript" => TS,
        "css" => CSS,
        "json" => JSON,
        "yaml" => YAML,
        "lua" => LUA,
        "sql" => SQL,
        "html" => HTML,
        "xml" => XML,
        "blade" => BLADE,
        "twig" => TWIG,
        "markdown" => MARKDOWN,
        "general" => GENERAL,
        _ => return None,
    })
}
