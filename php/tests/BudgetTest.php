<?php

declare(strict_types=1);

use Lattice\Money;
use Lattice\Promotions\Budget;

it("can be instantiated with an unlimited budget", function (): void {
    $budget = Budget::unlimited();

    expect($budget->applicationLimit)->toBeNull();
    expect($budget->monetaryLimit)->toBeNull();
});

it("can be instantiated with an application limit", function (): void {
    $budget = Budget::withApplicationLimit(100);

    expect($budget->applicationLimit)->toBe(100);
    expect($budget->monetaryLimit)->toBeNull();
});

it("can be instantiated with a monetary limit", function (): void {
    $budget = Budget::withMonetaryLimit(new Money(250_000, "GBP"));

    expect($budget->applicationLimit)->toBeNull();
    expect($budget->monetaryLimit)->toEqual(new Money(250_000, "GBP"));
});

it(
    "can be instantiated with both application and monetary limits",
    function (): void {
        $budget = Budget::withBothLimits(100, new Money(250_000, "GBP"));

        expect($budget->applicationLimit)->toBe(100);
        expect($budget->monetaryLimit)->toEqual(new Money(250_000, "GBP"));
    },
);
