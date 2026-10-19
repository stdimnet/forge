/**
 * Canned language server answers for the demo project, written the way Intelephense
 * formats them (markdown hover with a `<?php` signature block and `_@param_` tags).
 */
import type { EditorCompletionItem, Severity } from "@/ipc/lsp";

/** Inline code is written as ´code´ and fences as ~~~ so the docs fit in String.raw. */
const md = (s: string) => s.replace(/~~~/g, "```").replace(/´/g, "`").trim();

/** Hover docs by symbol. `name()` keys are methods/functions, plain keys everything else. */
export const hoverDocs: Record<string, string> = {
  "hasMany()": md(String.raw`
__Illuminate\Database\Eloquent\Concerns\HasRelationships::hasMany__

~~~php
<?php
public function hasMany(string $related, string|null $foreignKey = null, string|null $localKey = null): HasMany { }
~~~

Define a one-to-many relationship.

_@param_ ´class-string<TRelatedModel>´ ´$related´
_@param_ ´string|null´ ´$foreignKey´
_@param_ ´string|null´ ´$localKey´
_@return_ ´\Illuminate\Database\Eloquent\Relations\HasMany<TRelatedModel, $this>´
`),
  "belongsTo()": md(String.raw`
__Illuminate\Database\Eloquent\Concerns\HasRelationships::belongsTo__

~~~php
<?php
public function belongsTo(string $related, string|null $foreignKey = null, string|null $ownerKey = null, string|null $relation = null): BelongsTo { }
~~~

Define an inverse one-to-one or many relationship.

_@param_ ´class-string<TRelatedModel>´ ´$related´
_@param_ ´string|null´ ´$foreignKey´
_@param_ ´string|null´ ´$ownerKey´
_@param_ ´string|null´ ´$relation´
_@return_ ´\Illuminate\Database\Eloquent\Relations\BelongsTo<TRelatedModel, $this>´
`),
  orders: md(String.raw`
__App\Models\User::$orders__

~~~php
<?php
public readonly \Illuminate\Support\Collection<int, Order> $orders;
~~~

Magic property declared with ´@property-read´ in the PHPDoc of **User**. Resolved lazily through the ´orders()´ relationship.

_@var_ ´\Illuminate\Support\Collection<int, \App\Models\Order>´
`),
  "orders()": md(String.raw`
__App\Models\User::orders__

~~~php
<?php
public function orders(): HasMany { }
~~~

_@return_ ´\Illuminate\Database\Eloquent\Relations\HasMany´
`),
  "totalSpent()": md(String.raw`
__App\Models\User::totalSpent__

~~~php
<?php
public function totalSpent(): float { }
~~~

_@return_ ´float´
`),
  "casts()": md(String.raw`
__App\Models\User::casts__

~~~php
<?php
protected function casts(): array { }
~~~

Get the attributes that should be cast.

_@return_ ´array<string, string>´
`),
  Authenticatable: md(String.raw`
__Illuminate\Foundation\Auth\User__

~~~php
<?php
class User extends Model implements AuthenticatableContract, AuthorizableContract, CanResetPasswordContract { }
~~~

Base Eloquent model for users that can log in. Mixes in the ´Authenticatable´, ´Authorizable´, ´CanResetPassword´ and ´MustVerifyEmail´ traits.

_@see_ \Illuminate\Contracts\Auth\Authenticatable
`),
  User: md(String.raw`
__App\Models\User__

~~~php
<?php
class User extends Authenticatable { }
~~~

An authenticated customer of the shop.

_@property_ ´int´ ´$id´
_@property_ ´string´ ´$email´
_@property-read_ ´\Illuminate\Support\Collection<int, Order>´ ´$orders´
`),
  Order: md(String.raw`
__App\Models\Order__

~~~php
<?php
class Order extends Model { }
~~~
`),
  OrderStatus: md(String.raw`
__App\Models\OrderStatus__

~~~php
<?php
enum OrderStatus: string { }
~~~

_@see_ \App\Models\Order
`),
  Paid: md(String.raw`
__App\Models\OrderStatus::Paid__

~~~php
<?php
case Paid = 'paid';
~~~
`),
  HasMany: md(String.raw`
__Illuminate\Database\Eloquent\Relations\HasMany__

~~~php
<?php
class HasMany extends HasOneOrMany { }
~~~

_@template_ ´TRelatedModel of \Illuminate\Database\Eloquent\Model´
_@template_ ´TDeclaringModel of \Illuminate\Database\Eloquent\Model´
_@extends_ ´\Illuminate\Database\Eloquent\Relations\HasOneOrMany<TRelatedModel, TDeclaringModel>´
`),
  HasFactory: md(String.raw`
__Illuminate\Database\Eloquent\Factories\HasFactory__

~~~php
<?php
trait HasFactory { }
~~~

_@template_ ´TFactory of \Illuminate\Database\Eloquent\Factories\Factory´
`),
  Notifiable: md(String.raw`
__Illuminate\Notifications\Notifiable__

~~~php
<?php
trait Notifiable { }
~~~

Combines ´HasDatabaseNotifications´ and ´RoutesNotifications´ so the model can receive notifications.
`),
  "where()": md(String.raw`
__Illuminate\Support\Collection::where__

~~~php
<?php
public function where(callable|string $key, mixed $operator = null, mixed $value = null): static { }
~~~

Filter items by the given key value pair.

_@param_ ´callable|string´ ´$key´
_@param_ ´mixed´ ´$operator´
_@param_ ´mixed´ ´$value´
_@return_ ´static´
`),
  "sum()": md(String.raw`
__Illuminate\Support\Collection::sum__

~~~php
<?php
public function sum(callable|string|null $callback = null): mixed { }
~~~

Get the sum of the given values.

_@param_ ´(callable(TValue): mixed)|string|null´ ´$callback´
_@return_ ´mixed´
`),
  "array_map()": md(String.raw`
__array_map__

~~~php
<?php
function array_map(callable|null $callback, array $array, array ...$arrays): array { }
~~~

Applies the callback to the elements of the given arrays

_@param_ ´callable|null´ ´$callback´ Callback function to run for each element in each array.
_@param_ ´array´ ´$array´ An array to run through the callback function.
_@param_ ´array´ ´...$arrays´ Supplementary variable list of array arguments to run through the callback function.
_@return_ ´array´ an array containing all the elements of ´array´ after applying the callback function to each one.
_@link_ https://php.net/manual/en/function.array-map.php
`),
  "utf8_encode()": md(String.raw`
__utf8_encode__

~~~php
<?php
function utf8_encode(string $string): string { }
~~~

Converts a string from ISO-8859-1 to UTF-8

_@param_ ´string´ ´$string´ An ISO-8859-1 string.
_@return_ ´string´ the UTF-8 translation of ´string´.
_@deprecated_ 8.2 Use ´mb_convert_encoding()´ instead.
_@link_ https://php.net/manual/en/function.utf8-encode.php
`),
  "info()": md(String.raw`
__Illuminate\Support\Facades\Log::info__

~~~php
<?php
public static function info(string|\Stringable $message, array $context = []): void { }
~~~

Log an informational message to the logs.
`),
  Log: md(String.raw`
__Illuminate\Support\Facades\Log__

~~~php
<?php
class Log extends Facade { }
~~~

_@see_ \Illuminate\Log\LogManager
`),
  "response()": md(String.raw`
__response__

~~~php
<?php
function response(View|string|array|null $content = null, int $status = 200, array $headers = []): ResponseFactory|Response { }
~~~

Return a new response from the application.
`),
  "json()": md(String.raw`
__Illuminate\Contracts\Routing\ResponseFactory::json__

~~~php
<?php
public function json(mixed $data = [], int $status = 200, array $headers = [], int $options = 0): JsonResponse { }
~~~

Create a new JSON response instance.
`),
  "settle()": md(String.raw`
__App\Services\InvoiceService::settle__

~~~php
<?php
public function settle(Order $order): void { }
~~~
`),
  Checkout: md(String.raw`
__App\Services\Checkout__

~~~php
<?php
class Checkout { }
~~~
`),
};

export interface SignatureDef {
  label: string;
  params: string[];
  docs?: string[];
}

export const signatures: Record<string, SignatureDef> = {
  hasMany: {
    label: "hasMany(string $related, string|null $foreignKey = null, string|null $localKey = null): HasMany",
    params: ["string $related", "string|null $foreignKey = null", "string|null $localKey = null"],
  },
  belongsTo: {
    label: "belongsTo(string $related, string|null $foreignKey = null, string|null $ownerKey = null, string|null $relation = null): BelongsTo",
    params: ["string $related", "string|null $foreignKey = null", "string|null $ownerKey = null", "string|null $relation = null"],
  },
  where: {
    label: "where(callable|string $key, mixed $operator = null, mixed $value = null): static",
    params: ["callable|string $key", "mixed $operator = null", "mixed $value = null"],
  },
  sum: { label: "sum(callable|string|null $callback = null): mixed", params: ["callable|string|null $callback = null"] },
  array_map: {
    label: "array_map(callable|null $callback, array $array, array ...$arrays): array",
    params: ["callable|null $callback", "array $array", "array ...$arrays"],
  },
  utf8_encode: { label: "utf8_encode(string $string): string", params: ["string $string"] },
  info: { label: "info(string|\\Stringable $message, array $context = []): void", params: ["string|\\Stringable $message", "array $context = []"] },
  json: {
    label: "json(mixed $data = [], int $status = 200, array $headers = [], int $options = 0): JsonResponse",
    params: ["mixed $data = []", "int $status = 200", "array $headers = []", "int $options = 0"],
  },
  Checkout: {
    label: "__construct(string $currency = 'EUR', float $taxRate = 0.19)",
    params: ["string $currency = 'EUR'", "float $taxRate = 0.19"],
  },
  view: { label: "view(string|null $view = null, array $data = [], array $mergeData = []): View", params: ["string|null $view = null", "array $data = []", "array $mergeData = []"] },
  config: { label: "config(array|string|null $key = null, mixed $default = null): mixed", params: ["array|string|null $key = null", "mixed $default = null"] },
  count: { label: "count(Countable|array $value, int $mode = COUNT_NORMAL): int", params: ["Countable|array $value", "int $mode = COUNT_NORMAL"] },
  totalSpent: { label: "totalSpent(): float", params: [] },
};

type Item = Omit<EditorCompletionItem, "raw">;
const m = (label: string, detail: string, kind = "method"): Item => ({ label, kind, detail });

export const modelMembers: Item[] = [
  m("orders", "HasMany"),
  m("orders", "Collection<int, Order>", "property"),
  m("email", "string", "property"),
  m("id", "int", "property"),
  m("fillable", "list<string>", "property"),
  m("hidden", "array", "property"),
  m("totalSpent", "float"),
  m("hasMany", "HasMany"),
  m("hasOne", "HasOne"),
  m("belongsTo", "BelongsTo"),
  m("belongsToMany", "BelongsToMany"),
  m("save", "bool"),
  m("update", "bool"),
  m("delete", "bool|null"),
  m("fresh", "static|null"),
  m("refresh", "$this"),
  m("load", "$this"),
  m("notify", "void"),
  m("toArray", "array"),
  m("getKey", "mixed"),
  m("casts", "array"),
];

export const orderMembers: Item[] = [
  m("user", "BelongsTo"),
  m("status", "OrderStatus", "property"),
  m("total", "float", "property"),
  m("id", "int", "property"),
  m("save", "bool"),
  m("update", "bool"),
  m("load", "$this"),
  m("fresh", "static|null"),
  m("delete", "bool|null"),
];

export const collectionMembers: Item[] = [
  m("where", "static"),
  m("sum", "mixed"),
  m("map", "static"),
  m("filter", "static"),
  m("first", "TValue|null"),
  m("count", "int"),
  m("each", "$this"),
  m("pluck", "static"),
  m("sortBy", "static"),
  m("groupBy", "static"),
  m("isEmpty", "bool"),
  m("toArray", "array"),
];

export const staticMembers: Item[] = [
  { label: "class", kind: "keyword", detail: "class-string" },
  m("query", "Builder"),
  m("find", "static|null"),
  m("findOrFail", "static"),
  m("where", "Builder"),
  m("with", "Builder"),
  m("create", "static"),
  m("all", "Collection"),
  { label: "Paid", kind: "enumMember", detail: "OrderStatus" },
  { label: "Pending", kind: "enumMember", detail: "OrderStatus" },
  { label: "Refunded", kind: "enumMember", detail: "OrderStatus" },
];

export const variables: Item[] = [
  { label: "$this", kind: "variable", detail: "User" },
  { label: "$order", kind: "variable", detail: "Order" },
  { label: "$user", kind: "variable", detail: "User" },
  { label: "$request", kind: "variable", detail: "Request" },
];

export const globals: Item[] = [
  { label: "array_map", kind: "function", detail: "array" },
  { label: "array_filter", kind: "function", detail: "array" },
  { label: "array_key_exists", kind: "function", detail: "bool" },
  { label: "array_merge", kind: "function", detail: "array" },
  { label: "array_values", kind: "function", detail: "array" },
  { label: "abort", kind: "function", detail: "never" },
  { label: "collect", kind: "function", detail: "Collection" },
  { label: "config", kind: "function", detail: "mixed" },
  { label: "count", kind: "function", detail: "int" },
  { label: "response", kind: "function", detail: "ResponseFactory" },
  { label: "view", kind: "function", detail: "View" },
  { label: "Order", kind: "class", detail: "App\\Models" },
  { label: "OrderStatus", kind: "enum", detail: "App\\Models" },
  { label: "User", kind: "class", detail: "App\\Models" },
  { label: "Collection", kind: "class", detail: "Illuminate\\Support" },
  { label: "HasMany", kind: "class", detail: "Illuminate\\Database\\Eloquent\\Relations" },
  { label: "Authenticatable", kind: "interface", detail: "Illuminate\\Contracts\\Auth" },
  { label: "PHP_EOL", kind: "constant", detail: "string" },
  { label: "return", kind: "keyword" },
  { label: "function", kind: "keyword" },
  { label: "foreach", kind: "keyword" },
  { label: "fn", kind: "keyword" },
  { label: "utf8_encode", kind: "function", detail: "string", deprecated: true },
];

export interface DiagnosticRule {
  file: string;
  /** Text to find; the diagnostic covers `part` inside it (default: all of it). */
  find: string;
  part?: string;
  severity: Severity;
  message: string;
  unnecessary?: boolean;
  deprecated?: boolean;
}

export const diagnosticRules: DiagnosticRule[] = [
  {
    file: "app/Services/InvoiceService.php",
    find: "use Illuminate\\Support\\Carbon;",
    severity: "weak",
    message: "Import 'Carbon' is never used",
    unnecessary: true,
  },
  {
    file: "app/Services/InvoiceService.php",
    find: "private readonly Checkout $checkout",
    part: "$checkout",
    severity: "weak",
    message: "Private property '$checkout' is never used",
    unnecessary: true,
  },
  {
    file: "app/Services/InvoiceService.php",
    find: "$order->markAsPaid()",
    part: "markAsPaid",
    severity: "error",
    message: "Method 'markAsPaid' not found in \\App\\Models\\Order",
  },
  {
    file: "app/Services/InvoiceService.php",
    find: "$lines = ",
    part: "$lines",
    severity: "weak",
    message: "Unused local variable '$lines'. The value of the variable is not used anywhere.",
    unnecessary: true,
  },
  {
    file: "app/Services/InvoiceService.php",
    find: "$order->lines)",
    part: "lines",
    severity: "warning",
    message: "Property accessed via magic method",
  },
  {
    file: "app/Services/InvoiceService.php",
    find: "utf8_encode(",
    part: "utf8_encode",
    severity: "weak",
    message: "'utf8_encode' is deprecated since 8.2. Use mb_convert_encoding() instead.",
    deprecated: true,
  },
  {
    file: "app/Http/Controllers/OrderController.php",
    find: "$this->authorize(",
    part: "authorize",
    severity: "error",
    message: "Method 'authorize' not found in \\App\\Http\\Controllers\\OrderController",
  },
];
