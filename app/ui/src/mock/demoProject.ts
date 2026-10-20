/** In-memory demo project used when the UI runs without the Rust backend. */
export const demoProjectName = "acme-shop";

export const demoFiles: Record<string, string> = {
  "composer.json": `{
    "name": "acme/shop",
    "type": "project",
    "require": {
        "php": "^8.3",
        "laravel/framework": "^12.0",
        "doctrine/dbal": "^4.0"
    },
    "autoload": { "psr-4": { "App\\\\": "app/" } }
}
`,
  "artisan": "#!/usr/bin/env php\n<?php\n\nrequire __DIR__.'/vendor/autoload.php';\n",
  ".env.example": "APP_NAME=AcmeShop\nAPP_ENV=local\nDB_CONNECTION=mysql\nDB_HOST=127.0.0.1\nDB_DATABASE=shop\n",
  "README.md": "# Acme Shop\n\nDemo project for Forge.\n",
  "app/Models/User.php": `<?php

declare(strict_types=1);

namespace App\\Models;

use Illuminate\\Database\\Eloquent\\Factories\\HasFactory;
use Illuminate\\Database\\Eloquent\\Relations\\HasMany;
use Illuminate\\Foundation\\Auth\\User as Authenticatable;
use Illuminate\\Notifications\\Notifiable;

/**
 * An authenticated customer of the shop.
 *
 * @property int $id
 * @property string $email
 * @property-read \\Illuminate\\Support\\Collection<int, Order> $orders
 */
class User extends Authenticatable
{
    use HasFactory, Notifiable;

    /** @var list<string> */
    protected $fillable = ['name', 'email', 'password'];

    protected $hidden = ['password', 'remember_token'];

    public function orders(): HasMany
    {
        return $this->hasMany(Order::class);
    }

    public function totalSpent(): float
    {
        return $this->orders
            ->where('status', OrderStatus::Paid)
            ->sum(fn (Order $order) => $order->total);
    }

    protected function casts(): array
    {
        return [
            'email_verified_at' => 'datetime',
            'password' => 'hashed',
        ];
    }
}
`,
  "app/Models/Order.php": `<?php

declare(strict_types=1);

namespace App\\Models;

use Illuminate\\Database\\Eloquent\\Model;
use Illuminate\\Database\\Eloquent\\Relations\\BelongsTo;

class Order extends Model
{
    protected $casts = ['status' => OrderStatus::class];

    public function user(): BelongsTo
    {
        return $this->belongsTo(User::class);
    }
}
`,
  "app/Models/OrderStatus.php": `<?php

namespace App\\Models;

enum OrderStatus: string
{
    case Pending = 'pending';
    case Paid = 'paid';
    case Refunded = 'refunded';
}
`,
  "app/Http/Controllers/OrderController.php": `<?php

namespace App\\Http\\Controllers;

use App\\Models\\Order;
use App\\Services\\InvoiceService;
use Illuminate\\Http\\JsonResponse;
use Illuminate\\Http\\Request;
use Symfony\\Component\\Routing\\Attribute\\Route;

final class OrderController extends Controller
{
    public function __construct(private readonly InvoiceService $invoices)
    {
    }

    #[Route('/orders/{order}', name: 'orders.show', methods: ['GET'])]
    public function show(Request $request, Order $order): JsonResponse
    {
        $this->authorize('view', $order);

        $user = $request->user();
        $lines = $order->lines()->with('product')->get();
        $total = $lines->sum(fn ($line) => $line->price * $line->quantity);
        $discount = $user->isVip() ? 0.1 : 0.0;

        if ($order->isPayable()) {
            $this->invoices->settle($order);
        }

        return response()->json([
            'order' => $order->load('user'),
            'total' => round($total * (1 - $discount), 2),
        ]);
    }
}
`,
  "app/Http/Controllers/Controller.php": "<?php\n\nnamespace App\\Http\\Controllers;\n\nabstract class Controller\n{\n}\n",
  "app/Services/Checkout.php": `<?php

namespace App\\Services;

class Checkout
{
    private string $currency;
    private float $taxRate;

    public function __construct(string $currency = 'EUR', float $taxRate = 0.19)
    {
        $this->currency = $currency;
        $this->taxRate = $taxRate;
    }
}
`,
  "app/Services/InvoiceService.php": `<?php

declare(strict_types=1);

namespace App\\Services;

use App\\Models\\Order;
use App\\Models\\OrderStatus;
use Illuminate\\Support\\Carbon;
use Illuminate\\Support\\Facades\\Log;

final class InvoiceService
{
    public function __construct(private readonly Checkout $checkout)
    {
    }

    public function settle(Order $order): void
    {
        if ($order->status === OrderStatus::Paid) {
            return;
        }

        $order->markAsPaid();
        $lines = array_map(fn ($line) => $line->price * $line->quantity, $order->lines);

        Log::info(utf8_encode('Order settled'), ['order' => $order->id]);
    }
}
`,
  "public/index.php": `<?php

use Illuminate\\Foundation\\Application;
use Illuminate\\Http\\Request;

define('LARAVEL_START', microtime(true));

// Determine if the application is in maintenance mode...
if (file_exists($maintenance = __DIR__.'/../storage/framework/maintenance.php')) {
    require $maintenance;
}

// Register the Composer autoloader...
require __DIR__.'/../vendor/autoload.php';

// Bootstrap Laravel and handle the request...
/** @var Application $app */
$app = require_once __DIR__.'/../bootstrap/app.php';

$app->handleRequest(Request::capture());
`,
  "routes/web.php": `<?php

use App\\Http\\Controllers\\OrderController;
use Illuminate\\Support\\Facades\\Route;

Route::get('/', fn () => view('welcome'));
Route::get('/orders/{order}', [OrderController::class, 'show'])->name('orders.show');
`,
  "resources/views/welcome.blade.php": "<x-layout>\n    <h1>{{ config('app.name') }}</h1>\n</x-layout>\n",
  "database/migrations/2026_01_01_000000_create_orders_table.php": "<?php\n\nreturn new class extends Migration {\n    public function up(): void\n    {\n        Schema::create('orders', function (Blueprint $table) {\n            $table->id();\n            $table->foreignId('user_id')->constrained();\n            $table->string('status');\n            $table->decimal('total', 10, 2);\n            $table->timestamps();\n        });\n    }\n};\n",
  "tests/Feature/OrderTest.php": "<?php\n\nnamespace Tests\\Feature;\n\nuse Tests\\TestCase;\n\nclass OrderTest extends TestCase\n{\n    public function test_user_can_view_order(): void\n    {\n        $this->assertTrue(true);\n    }\n}\n",
  "tests/Unit/UserTest.php": `<?php

namespace Tests\\Unit;

use App\\Models\\User;
use PHPUnit\\Framework\\Attributes\\Test;
use PHPUnit\\Framework\\TestCase;

class UserTest extends TestCase
{
    public function test_full_name_joins_first_and_last_name(): void
    {
        $user = new User(['first_name' => 'Ada', 'last_name' => 'Lovelace']);

        $this->assertSame('Ada Lovelace', $user->fullName());
    }

    public function test_display_name_prefers_nickname(): void
    {
        $user = new User(['first_name' => 'Grace', 'nickname' => 'Amazing Grace']);

        $this->assertSame('Amazing Grace', $user->displayName());
    }

    #[Test]
    public function email_is_normalized(): void
    {
        $user = new User(['email' => ' Ada@Example.COM ']);

        $this->assertSame('ada@example.com', $user->email);
    }

    public function test_orders_relation_is_empty_for_new_users(): void
    {
        $this->assertCount(0, (new User())->orders);
    }
}
`,
  "tests/Unit/CheckoutTest.php": `<?php

namespace Tests\\Unit;

use App\\Services\\Checkout;
use PHPUnit\\Framework\\TestCase;

class CheckoutTest extends TestCase
{
    public function test_total_includes_tax(): void
    {
        $this->assertSame(119.0, (new Checkout())->total(100.0, 0.19));
    }

    public function test_empty_cart_cannot_be_checked_out(): void
    {
        $this->markTestSkipped('Cart rules move to the Cart service.');
    }
}
`,
};
