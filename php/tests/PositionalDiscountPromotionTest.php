<?php

declare(strict_types=1);

use Lattice\Discount\Percentage;
use Lattice\Discount\Simple;
use Lattice\Item;
use Lattice\Money;
use Lattice\Product;
use Lattice\Promotion\Budget;
use Lattice\Promotion\PromotionInterface;
use Lattice\Promotion\Positional;
use Lattice\Qualification;
use Lattice\Stack\Layer;
use Lattice\Stack\LayerOutput;
use Lattice\Stack\StackBuilder;

it("implements Promotion interface", function () {
    $promotion = new Positional(
        reference: 123,
        size: 3,
        positions: [2],
        qualification: Qualification::matchAll(),
        discount: Simple::amountOff(new Money(5_00, "GBP")),
        budget: Budget::unlimited(),
    );

    expect($promotion)->toBeInstanceOf(PromotionInterface::class);
});

it("can be instantiated", function () {
    $promotion = new Positional(
        reference: 123,
        size: 3,
        positions: [2],
        qualification: Qualification::matchAll(),
        discount: Simple::amountOff(new Money(5_00, "GBP")),
        budget: Budget::unlimited(),
    );

    expect($promotion->reference)->toBe(123);
    expect($promotion->size)->toBe(3);
    expect($promotion->positions)->toBe([2]);
    expect($promotion->discount)->toBeInstanceOf(Simple::class);
    expect($promotion->discount->amount)->toEqual(new Money(5_00, "GBP"));
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
            tags: [],
        ),
    );

    $item2 = Item::fromProduct(
        reference: "item-2",
        product: new Product(
            reference: "product-2",
            name: "Product 2",
            price: new Money(7_00, "GBP"),
            tags: [],
        ),
    );

    $item3 = Item::fromProduct(
        reference: "item-3",
        product: new Product(
            reference: "product-3",
            name: "Product 3",
            price: new Money(5_00, "GBP"),
            tags: [],
        ),
    );

    $promotion = new Positional(
        reference: "promotion",
        qualification: Qualification::matchAll(),
        size: 3,
        positions: [2],
        discount: Simple::percentageOff(Percentage::fromDecimal(1.0)),
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

    $receipt = $stack->build()->process([$item1, $item2, $item3]);

    expect($receipt->subtotal)->toEqual(new Money(15_00, "GBP"));
    expect($receipt->total)->toEqual(new Money(12_00, "GBP"));
});
