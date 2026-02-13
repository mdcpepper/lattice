<?php

declare(strict_types=1);

use Lattice\Money;

it("can be created with amount for valid currency", function (): void {
    assertLatticeExtensionLoaded();

    $money = new Money(123, "GBP");

    expect($money->amount)->toBe(123);
    expect($money->currency)->toBe("GBP");
});

it("throws an exception with invalid currency", function (): void {
    assertLatticeExtensionLoaded();

    $money = new Money(123, "ABC");
})->throws(Exception::class, "Invalid currency.");
