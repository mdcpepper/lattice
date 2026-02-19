<?php

declare(strict_types=1);

use Lattice\Discount\Percentage;
use Lattice\Item;
use Lattice\Money;
use Lattice\Product;
use Lattice\Promotion\Budget;
use Lattice\Promotion\PromotionInterface;
use Lattice\Promotion\TieredThreshold\Discount;
use Lattice\Promotion\TieredThreshold\Threshold;
use Lattice\Promotion\TieredThreshold\Tier;
use Lattice\Promotion\TieredThreshold\TieredThreshold;
use Lattice\Qualification;
use Lattice\Stack\Layer;
use Lattice\Stack\LayerOutput;
use Lattice\Stack\StackBuilder;

it("implements Promotion interface", function () {
    $promotion = new TieredThreshold(
        reference: 123,
        tiers: [
            new Tier(
                Threshold::withMonetaryThreshold(new Money(3_00, "GBP")),
                null,
                Qualification::matchAny(["wine"]),
                Qualification::matchAny(["cheese"]),
                Discount::percentageOffEachItem(Percentage::fromDecimal(0.1)),
            ),
        ],
        budget: Budget::unlimited(),
    );

    expect($promotion)->toBeInstanceOf(PromotionInterface::class);
});

it("can be instantiated", function () {
    $tier = new Tier(
        Threshold::withBothThresholds(new Money(3_00, "GBP"), 2),
        Threshold::withMonetaryThreshold(new Money(10_00, "GBP")),
        Qualification::matchAny(["wine"]),
        Qualification::matchAny(["cheese"]),
        Discount::amountOffEachItem(new Money(1_00, "GBP")),
    );

    $promotion = new TieredThreshold(
        reference: 123,
        tiers: [$tier],
        budget: Budget::unlimited(),
    );

    expect($promotion->reference)->toBe(123);
    expect($promotion->tiers)->toHaveCount(1);
    expect($promotion->tiers[0])->toBeInstanceOf(Tier::class);
    expect($promotion->tiers[0]->lowerThreshold->monetaryThreshold)->toEqual(
        new Money(3_00, "GBP"),
    );
    expect($promotion->tiers[0]->lowerThreshold->itemCountThreshold)->toBe(2);
    expect($promotion->tiers[0]->upperThreshold->monetaryThreshold)->toEqual(
        new Money(10_00, "GBP"),
    );
    expect($promotion->tiers[0]->discount)->toBeInstanceOf(Discount::class);
    expect($promotion->tiers[0]->discount->amount)->toEqual(
        new Money(1_00, "GBP"),
    );
    expect($promotion->budget->redemptionLimit)->toBeNull();
    expect($promotion->budget->monetaryLimit)->toBeNull();
});

it("applies discount correctly", function () {
    $wine = Item::fromProduct(
        reference: "wine-item",
        product: new Product(
            reference: "wine",
            name: "Wine",
            price: new Money(3_00, "GBP"),
            tags: ["wine"],
        ),
    );

    $cheese = Item::fromProduct(
        reference: "cheese-item",
        product: new Product(
            reference: "cheese",
            name: "Cheese",
            price: new Money(2_00, "GBP"),
            tags: ["cheese"],
        ),
    );

    // 50% off cheese when you spend at least £3 on wine
    $promotion = new TieredThreshold(
        reference: "promotion",
        tiers: [
            new Tier(
                Threshold::withMonetaryThreshold(new Money(3_00, "GBP")),
                null,
                Qualification::matchAny(["wine"]),
                Qualification::matchAny(["cheese"]),
                Discount::percentageOffEachItem(Percentage::fromDecimal(0.5)),
            ),
        ],
        budget: Budget::unlimited(),
    );

    $stack = new StackBuilder();

    $stack->addLayer(
        new Layer(
            reference: "layer",
            output: LayerOutput::passThrough(),
            promotions: [$promotion],
        ),
    );

    $receipt = $stack->build()->process([$wine, $cheese]);

    expect($receipt->subtotal)->toEqual(new Money(5_00, "GBP"));
    expect($receipt->total)->toEqual(new Money(4_00, "GBP"));
});
