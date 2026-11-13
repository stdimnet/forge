<?php

namespace App\Providers;

use Illuminate\Support\Facades\Blade;
use Illuminate\Support\ServiceProvider;

class AppServiceProvider extends ServiceProvider
{
    public function boot(): void
    {
        Blade::directive('money', fn ($expr) => "<?php echo number_format($expr, 2); ?>");
        Blade::if('admin', fn () => auth()->user()?->isAdmin());
    }
}
