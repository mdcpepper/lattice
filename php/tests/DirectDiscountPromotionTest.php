<?php

declare(strict_types=1);

use Lattice\Discount\SimpleDiscount;
use Lattice\Money;
use Lattice\Promotions\Budget;
use Lattice\Promotions\DirectDiscount;
use Lattice\Promotions\Promotion;
use Lattice\Qualification;

it("implements Promotion interface", function () {
    assertLatticeExtensionLoaded();

    $promotion = new DirectDiscount(
        reference: 123,
        qualification: Qualification::matchAll(),
        discount: SimpleDiscount::amountOff(new Money(123, "GBP")),
        budget: Budget::unlimited(),
    );

    expect($promotion)->toBeInstanceOf(Promotion::class);
});

it("can be instantiated", function () {
    assertLatticeExtensionLoaded();

    $promotion = new DirectDiscount(
        reference: 123,
        qualification: Qualification::matchAll(),
        discount: SimpleDiscount::amountOff(new Money(123, "GBP")),
        budget: Budget::unlimited(),
    );

    expect($promotion->reference)->toBe(123);
    expect($promotion->discount->amount)->toEqual(new Money(123, "GBP"));
    expect($promotion->budget->applicationLimit)->toBeNull();
    expect($promotion->budget->monetaryLimit)->toBeNull();
    expect($promotion)->toBeInstanceOf(Promotion::class);
});
