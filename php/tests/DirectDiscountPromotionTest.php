<?php

declare(strict_types=1);

use FeedCode\Lattice\Discount\SimpleDiscount;
use FeedCode\Lattice\Money;
use FeedCode\Lattice\Promotions\Budget;
use FeedCode\Lattice\Promotions\DirectDiscount;
use FeedCode\Lattice\Promotions\Promotion;
use FeedCode\Lattice\Qualification;

it("can be instantiated", function () {
    assertLatticeExtensionLoaded();

    $promotion = new DirectDiscount(
        key: 123,
        qualification: Qualification::matchAll(),
        discount: SimpleDiscount::amountOff(new Money(123, "GBP")),
        budget: Budget::unlimited(),
    );

    expect($promotion->key)->toBe(123);
    expect($promotion->discount->amount)->toEqual(new Money(123, "GBP"));
    expect($promotion->budget->applicationLimit)->toBeNull();
    expect($promotion->budget->monetaryLimit)->toBeNull();
    expect($promotion)->toBeInstanceOf(Promotion::class);
});
