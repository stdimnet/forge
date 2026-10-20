<?php

use App\Http\Controllers\Admin\DashboardController;
use App\Http\Controllers\OrderController;
use App\Http\Controllers\PhotoController;
use App\Http\Controllers\ShowProfile;
use Illuminate\Support\Facades\Route;

Route::get('/', fn () => view('welcome'))->name('home');
Route::view('/about', 'pages.about')->name('about');
Route::redirect('/here', '/there');

Route::get('/orders/{order}', [OrderController::class, 'show'])->name('orders.show')->middleware('can:view,order');
Route::match(['get', 'post'], '/checkout', [OrderController::class, 'checkout'])->name('checkout');
Route::get('/profile', ShowProfile::class)->middleware(['auth', 'verified']);

Route::prefix('admin')->name('admin.')->middleware('auth')->group(function () {
    Route::get('/', [DashboardController::class, 'index'])->name('dashboard');
    Route::resource('photos', PhotoController::class)->only(['index', 'show']);
});

Route::controller(OrderController::class)->prefix('shop')->group(function () {
    Route::post('/orders', 'store')->name('orders.store');
});

Route::group(['prefix' => 'legacy', 'as' => 'legacy.'], function () {
    Route::get('/report', [OrderController::class, 'report'])->name('report');
});

require __DIR__.'/auth.php';
