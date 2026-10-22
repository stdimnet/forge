<?php

namespace App\Services;

use App\Models\Order;
use App\Mail\Invoice;

class OrderService
{
    public function place(Customer $customer, array $items): Order
    {
        $order = new Order(['customer_id' => $customer->id]);
        foreach ($items as $item) {
            $order->addItem($item);
        }
        if ($order->total() > 1000) {
            $order->flag('review');
        }
        $order->save();
        $mail = new Invoice($order, true);
        try {
            $mail->send();
        } catch (\RuntimeException $e) {
        }
        try {
            $this->log('placed');
        } finally {
            $this->flush();
        }
        echo 'done', PHP_EOL;
        return $order;
    }

    private function log(string $message): void
    {
        // $order->save() in a comment is not code
        logger()->info($message);
    }
}
