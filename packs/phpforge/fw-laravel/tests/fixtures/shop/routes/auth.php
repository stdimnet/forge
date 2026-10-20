<?php

use Illuminate\Support\Facades\Route;

Route::middleware('guest')->group(function () {
    Route::get('login', fn () => view('auth.login'))->name('login');
});
