<?php

namespace App\Console\Commands;

use Illuminate\Console\Command;

class PruneOrders extends Command
{
    protected $signature = 'orders:prune {--days=30 : Keep orders newer than this}';

    protected $description = 'Delete abandoned orders';

    public function handle(): int
    {
        return self::SUCCESS;
    }
}
