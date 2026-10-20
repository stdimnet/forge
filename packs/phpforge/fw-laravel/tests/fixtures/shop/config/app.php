<?php

return [
    'name' => env('APP_NAME', 'Laravel'),
    'locale' => env('APP_LOCALE', 'en'),
    'debug' => (bool) env('APP_DEBUG', false),
    'maintenance' => [
        'driver' => 'file',
    ],
];
