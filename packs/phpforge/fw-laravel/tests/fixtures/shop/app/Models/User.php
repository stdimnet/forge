<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Casts\Attribute;
use Illuminate\Database\Eloquent\Relations\HasMany;
use Illuminate\Foundation\Auth\User as Authenticatable;

/**
 * @property-read string $avatar_url
 */
class User extends Authenticatable
{
    protected $fillable = ['name', 'email', 'password'];

    protected function casts(): array
    {
        return [
            'email_verified_at' => 'datetime',
            'password' => 'hashed',
        ];
    }

    public function orders(): HasMany
    {
        return $this->hasMany(Order::class);
    }

    public function scopeActive(Builder $query): void
    {
        $query->whereNotNull('email_verified_at');
    }

    protected function fullName(): Attribute
    {
        return Attribute::get(fn () => $this->name);
    }

    public function getInitialsAttribute(): string
    {
        return '';
    }
}
