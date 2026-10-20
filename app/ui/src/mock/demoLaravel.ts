/**
 * Laravel parts of the demo project: controllers, a model, migrations, config, translations,
 * Blade views and components, so the Laravel plugin has a realistic project in browser mode.
 */
export const demoLaravelFiles: Record<string, string> = {
  "routes/api.php": `<?php

use App\\Http\\Controllers\\ProductController;
use Illuminate\\Http\\Request;
use Illuminate\\Support\\Facades\\Route;

Route::get('/user', fn (Request $request) => $request->user())->middleware('auth:sanctum');

Route::apiResource('products', ProductController::class)->only(['index', 'show']);
`,
  "app/Http/Controllers/CheckoutController.php": `<?php

namespace App\\Http\\Controllers;

use App\\Models\\Order;
use App\\Models\\OrderStatus;
use Illuminate\\Http\\RedirectResponse;
use Illuminate\\Http\\Request;
use Illuminate\\View\\View;

class CheckoutController extends Controller
{
    public function create(Request $request): View
    {
        $pending = Order::query()
            ->where('status', OrderStatus::Pending)
            ->latest()
            ->first();

        return view('checkout.create', ['order' => $pending]);
    }

    public function store(Request $request): RedirectResponse
    {
        $order = $request->user()->orders()->create([
            'status' => OrderStatus::Pending,
            'total' => $request->input('total'),
        ]);

        return redirect()->route('orders.show', $order);
    }
}
`,
  "app/Http/Controllers/ProductController.php": `<?php

namespace App\\Http\\Controllers;

use App\\Models\\Product;
use Illuminate\\View\\View;

class ProductController extends Controller
{
    public function index(): View
    {
        $products = Product::available()->orderBy('name')->paginate(24);

        return view('products.index', compact('products'));
    }

    public function show(Product $product): View
    {
        return view('products.show', ['product' => $product]);
    }
}
`,
  "app/Http/Controllers/Admin/DashboardController.php": `<?php

namespace App\\Http\\Controllers\\Admin;

use App\\Http\\Controllers\\Controller;
use App\\Models\\Order;
use Illuminate\\View\\View;

class DashboardController extends Controller
{
    public function index(): View
    {
        return view('admin.dashboard', [
            'revenue' => Order::query()->sum('total'),
        ]);
    }
}
`,
  "app/Models/Product.php": `<?php

namespace App\\Models;

use Illuminate\\Database\\Eloquent\\Builder;
use Illuminate\\Database\\Eloquent\\Model;

class Product extends Model
{
    protected $fillable = ['name', 'slug', 'price', 'stock'];

    protected $casts = ['price' => 'decimal:2', 'published_at' => 'datetime'];

    public function scopeAvailable(Builder $query): void
    {
        $query->where('stock', '>', 0);
    }
}
`,
  "app/Providers/AppServiceProvider.php": `<?php

namespace App\\Providers;

use Illuminate\\Support\\Facades\\Blade;
use Illuminate\\Support\\ServiceProvider;

class AppServiceProvider extends ServiceProvider
{
    public function boot(): void
    {
        Blade::directive('money', fn (string $amount) => "<?php echo number_format($amount, 2); ?>");
    }
}
`,
  "app/View/Components/Button.php": `<?php

namespace App\\View\\Components;

use Illuminate\\View\\Component;

class Button extends Component
{
    public function __construct(public string $variant = 'primary')
    {
    }

    public function render()
    {
        return view('components.button');
    }
}
`,
  "database/migrations/0001_01_01_000000_create_users_table.php": `<?php

use Illuminate\\Database\\Migrations\\Migration;
use Illuminate\\Database\\Schema\\Blueprint;
use Illuminate\\Support\\Facades\\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('users', function (Blueprint $table) {
            $table->id();
            $table->string('name');
            $table->string('email')->unique();
            $table->timestamp('email_verified_at')->nullable();
            $table->string('password');
            $table->rememberToken();
            $table->timestamps();
        });
    }
};
`,
  "database/migrations/2026_01_02_000000_create_products_table.php": `<?php

use Illuminate\\Database\\Migrations\\Migration;
use Illuminate\\Database\\Schema\\Blueprint;
use Illuminate\\Support\\Facades\\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('products', function (Blueprint $table) {
            $table->id();
            $table->string('name');
            $table->string('slug')->unique();
            $table->decimal('price', 10, 2);
            $table->unsignedInteger('stock')->default(0);
            $table->timestamp('published_at')->nullable();
            $table->timestamps();
        });
    }
};
`,
  "config/app.php": `<?php

return [
    'name' => env('APP_NAME', 'Laravel'),
    'env' => env('APP_ENV', 'production'),
    'debug' => (bool) env('APP_DEBUG', false),
    'url' => env('APP_URL', 'http://localhost'),
    'timezone' => 'UTC',
    'locale' => env('APP_LOCALE', 'en'),
    'fallback_locale' => env('APP_FALLBACK_LOCALE', 'en'),
    'maintenance' => [
        'driver' => env('APP_MAINTENANCE_DRIVER', 'file'),
    ],
];
`,
  "config/services.php": `<?php

return [
    'stripe' => [
        'key' => env('STRIPE_KEY'),
        'secret' => env('STRIPE_SECRET'),
    ],
    'postmark' => [
        'token' => env('POSTMARK_TOKEN'),
    ],
];
`,
  "config/shop.php": `<?php

return [
    'currency' => 'EUR',
    'tax_rate' => 0.19,
    'pagination' => [
        'products' => 24,
        'orders' => 15,
    ],
];
`,
  "lang/en/messages.php": `<?php

return [
    'welcome' => 'Welcome to Acme Shop',
    'cart' => [
        'empty' => 'Your cart is empty.',
        'checkout' => 'Proceed to checkout',
    ],
    'order' => [
        'paid' => 'Order :number has been paid.',
    ],
];
`,
  "lang/de/messages.php": `<?php

return [
    'welcome' => 'Willkommen bei Acme Shop',
];
`,
  "resources/views/layouts/app.blade.php": `<!DOCTYPE html>
<html lang="{{ str_replace('_', '-', app()->getLocale()) }}">
<head>
    <meta charset="utf-8">
    <title>@yield('title', config('app.name'))</title>
    @vite(['resources/css/app.css', 'resources/js/app.ts'])
</head>
<body class="antialiased">
    @include('partials.nav')
    <main>
        @yield('content')
    </main>
</body>
</html>
`,
  "resources/views/partials/nav.blade.php": `<nav>
    <a href="{{ route('home') }}">{{ config('app.name') }}</a>
    <a href="{{ route('products.index') }}">Products</a>
    @auth
        <a href="{{ route('checkout.create') }}">{{ __('messages.cart.checkout') }}</a>
    @endauth
</nav>
`,
  "resources/views/orders/show.blade.php": `@extends('layouts.app')

@section('title', 'Order #' . $order->id)

@section('content')
    {{-- Order summary with its lines --}}
    <x-alert type="success" :message="__('messages.order.paid', ['number' => $order->id])" />

    <h1 class="text-2xl font-semibold">Order #{{ $order->id }}</h1>
    <p>Status: <strong>{{ $order->status->name }}</strong>, placed {{ $order->created_at->diffForHumans() }}</p>

    @forelse ($order->lines as $line)
        <div @class(['line', 'line--free' => $line->price == 0])>
            {{ $line->quantity }} &times; {{ $line->product->name }}
            <span>@money($line->price * $line->quantity)</span>
        </div>
    @empty
        <p>{{ __('messages.cart.empty') }}</p>
    @endforelse

    @php
        $total = $order->lines->sum(fn ($line) => $line->price * $line->quantity);
    @endphp

    {!! $order->notes_html !!}

    @can('update', $order)
        <form method="POST" action="{{ route('checkout.store') }}">
            @csrf
            <x-button variant="primary">Pay {{ number_format($total, 2) }} €</x-button>
        </form>
    @endcan
@endsection

@push('scripts')
    <script>
        const orderId = {{ $order->id }};
        console.log('Order loaded', orderId);
    </script>
@endpush
`,
  "resources/views/products/index.blade.php": `@extends('layouts.app')

@section('content')
    @foreach ($products as $product)
        <a href="{{ route('products.show', $product) }}">{{ $product->name }}</a>
    @endforeach

    {{ $products->links() }}
@endsection
`,
  "resources/views/products/show.blade.php": `@extends('layouts.app')

@section('content')
    <h1>{{ $product->name }}</h1>
    <p>@money($product->price)</p>
@endsection
`,
  "resources/views/checkout/create.blade.php": `@extends('layouts.app')

@section('content')
    <h1>{{ __('messages.cart.checkout') }}</h1>
@endsection
`,
  "resources/views/pages/about.blade.php": `@extends('layouts.app')

@section('content')
    <h1>About {{ config('app.name') }}</h1>
@endsection
`,
  "resources/views/admin/dashboard.blade.php": `@extends('layouts.app')

@section('content')
    <h1>Revenue: @money($revenue)</h1>
@endsection
`,
  "resources/views/components/alert.blade.php": `@props(['type' => 'info', 'message'])

<div {{ $attributes->merge(['class' => 'alert alert-' . $type]) }} role="alert">
    {{ $message ?? $slot }}
</div>
`,
  "resources/views/components/layout.blade.php": `<html>
<body>
    {{ $slot }}
</body>
</html>
`,
  "resources/views/components/button.blade.php": `<button {{ $attributes->class(['btn', 'btn-' . $variant]) }}>
    {{ $slot }}
</button>
`,
};
