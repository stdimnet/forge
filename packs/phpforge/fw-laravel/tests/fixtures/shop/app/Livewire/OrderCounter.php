<?php

namespace App\Livewire;

use Livewire\Component;

class OrderCounter extends Component
{
    public int $count = 0;

    public function render()
    {
        return view('livewire.order-counter');
    }
}
