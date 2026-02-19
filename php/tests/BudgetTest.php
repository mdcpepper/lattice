<?php

declare(strict_types=1);

use Lattice\Money;
use Lattice\Promotion\Budget;

it("can be instantiated with an unlimited budget", function (): void {
    $budget = Budget::unlimited();

    expect($budget->redemptionLimit)->toBeNull();
    expect($budget->monetaryLimit)->toBeNull();
});

it("can be instantiated with an redemption limit", function (): void {
    $budget = Budget::withRedemptionLimit(100);

    expect($budget->redemptionLimit)->toBe(100);
    expect($budget->monetaryLimit)->toBeNull();
});

it("can be instantiated with a monetary limit", function (): void {
    $budget = Budget::withMonetaryLimit(new Money(250_000, "GBP"));

    expect($budget->redemptionLimit)->toBeNull();
    expect($budget->monetaryLimit)->toEqual(new Money(250_000, "GBP"));
});

it(
    "can be instantiated with both redemption and monetary limits",
    function (): void {
        $budget = Budget::withBothLimits(100, new Money(250_000, "GBP"));

        expect($budget->redemptionLimit)->toBe(100);
        expect($budget->monetaryLimit)->toEqual(new Money(250_000, "GBP"));
    },
);
