<?php

namespace App\Http;

use App\Container;
use App\Models\Order;
use App\Services\Mailer;

class Checkout
{
    public function handle(Container $container, Order $order): void
    {
        $container->get(Mailer::class)->send('a@b.c');
        $container->get('cache')->remember('totals');
        app('mailer');
        app(Mailer::class)->send('x@y.z');
        $mailer = $container->get('mailer');
        $mailer->send('c@d.e');
        tap(new Mailer())->send('f@g.h');
        $order->settle(10);
        $order->total();
        $order->setStatus('cancelled');
        $order->priority('urgent');
        $order->number = 'B-2';
        $shape = $order->toArray();
        $shape['mailer']->send('i@j.k');
        Responder::fail('stop');
        $order->pay(1);
    }
}
