<?php

return new class extends Migration
{
    public function up(): void
    {
        Schema::table('orders', function (Blueprint $table) {
            $table->dropColumn('notes');
            $table->softDeletes();
        });
    }
};
