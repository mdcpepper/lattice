<?php

declare(strict_types=1);

use Lattice\Discount\Percentage;
use Lattice\Item;
use Lattice\Money;
use Lattice\Product;
use Lattice\Promotion\Budget;
use Lattice\Promotion\PromotionInterface;
use Lattice\Promotion\MixAndMatch\Discount;
use Lattice\Promotion\MixAndMatch\MixAndMatch;
use Lattice\Promotion\MixAndMatch\Slot as MixAndMatchSlot;
use Lattice\Qualification;
use Lattice\Stack\Layer;
use Lattice\Stack\LayerOutput;
use Lattice\Stack\StackBuilder;

it("implements Promotion interface", function () {
    $promotion = new MixAndMatch(
        reference: 123,
        slots: [
            new MixAndMatchSlot(
                reference: "slot-1",
                qualification: Qualification::matchAll(),
                min: 2,
                max: 2,
            ),
        ],
        discount: Discount::percentageOffAllItems(
            Percentage::fromDecimal(0.25),
        ),
        budget: Budget::unlimited(),
    );

    expect($promotion)->toBeInstanceOf(PromotionInterface::class);
});

it("can be instantiated", function () {
    $slot = new MixAndMatchSlot(
        reference: "slot-1",
        qualification: Qualification::matchAll(),
        min: 2,
        max: 2,
    );

    $promotion = new MixAndMatch(
        reference: 123,
        slots: [$slot],
        discount: Discount::amountOffEachItem(new Money(1_00, "GBP")),
        budget: Budget::unlimited(),
    );

    expect($promotion->reference)->toBe(123);
    expect($promotion->slots)->toHaveCount(1);
    expect($promotion->slots[0])->toBeInstanceOf(MixAndMatchSlot::class);
    expect($promotion->slots[0]->min)->toBe(2);
    expect($promotion->slots[0]->max)->toBe(2);
    expect($promotion->discount)->toBeInstanceOf(Discount::class);
    expect($promotion->discount->amount)->toEqual(new Money(1_00, "GBP"));
    expect($promotion->budget->redemptionLimit)->toBeNull();
    expect($promotion->budget->monetaryLimit)->toBeNull();
});

it("applies discount correctly", function () {
    $item1 = Item::fromProduct(
        reference: "item-1",
        product: new Product(
            reference: "product-1",
            name: "Product 1",
            price: new Money(3_00, "GBP"),
            tags: ["eligible"],
        ),
    );

    $item2 = Item::fromProduct(
        reference: "item-2",
        product: new Product(
            reference: "product-2",
            name: "Product 2",
            price: new Money(1_00, "GBP"),
            tags: ["eligible"],
        ),
    );

    $promotion = new MixAndMatch(
        reference: "promotion",
        slots: [
            new MixAndMatchSlot(
                reference: "slot-1",
                qualification: Qualification::matchAny(["eligible"]),
                min: 2,
                max: 2,
            ),
        ],
        discount: Discount::percentageOffAllItems(Percentage::fromDecimal(0.5)),
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

    $receipt = $stack->build()->process([$item1, $item2]);

    expect($receipt->subtotal)->toEqual(new Money(4_00, "GBP"));
    expect($receipt->total)->toEqual(new Money(2_00, "GBP"));
});
