<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

class Order extends Model
{
    protected $casts = ['status' => OrderStatus::class, 'total' => 'decimal:2'];

    public function user()
    {
        return $this->belongsTo(User::class);
    }

    public function lines()
    {
        return $this->hasMany(OrderLine::class, 'order_id');
    }
}
