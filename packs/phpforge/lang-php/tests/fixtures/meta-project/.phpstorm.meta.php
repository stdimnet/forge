<?php

namespace PHPSTORM_META {

    use App\Services\Cache;

    override(\Psr\Container\ContainerInterface::get(0), map([
        '' => '@',
        'cache' => Cache::class,
        'mailer' => \App\Services\Mailer::class,
    ]));
    override(\app(0), map([
        '' => '@',
        'cache' => Cache::class,
    ]));
    override(\tap(0), type(0));
    override(\first_of(0), elementType(0));

    registerArgumentsSet('order_statuses', 'draft', 'paid', 'shipped');
    expectedArguments(\App\Models\Order::setStatus(), 0, argumentsSet('order_statuses'));
    expectedArguments(\App\Models\Order::sortBy(), 1, \App\Models\Order::ASC, \App\Models\Order::DESC);
    expectedArguments(\App\Services\Mailer::send(), 1, \App\Services\Mailer::HTML | \App\Services\Mailer::QUEUED);
    expectedReturnValues(\App\Models\Order::status(), argumentsSet('order_statuses'));
    exitPoint(\App\Http\Responder::fail());
}
