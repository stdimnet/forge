<?php

namespace App\Http\Controllers;

use App\Models\Order;
use App\Models\User;

class OrderController extends Controller
{
    public function index()
    {
        return Order::query()->where('status', 'paid')->get();
    }

    public function show(Order $order)
    {
        return view('orders.show', ['order' => $order]);
    }

    public function store()
    {
        return to_route('orders.show', 1);
    }

    public function checkout()
    {
        return redirect()->route('home');
    }

    public function report()
    {
        $user = User::find(1);
        return $user->orders()->where('total', '>', 10)->get();
    }

    public function update(Order $order)
    {
    }

    public function destroy(Order $order)
    {
    }
}
