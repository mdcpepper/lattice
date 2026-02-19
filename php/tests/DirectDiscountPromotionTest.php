<?php

declare(strict_types=1);

use Lattice\Discount\Percentage;
use Lattice\Discount\Simple;
use Lattice\Item;
use Lattice\Money;
use Lattice\Product;
use Lattice\Promotion\Budget;
use Lattice\Promotion\PromotionInterface;
use Lattice\Promotion\Direct;
use Lattice\Qualification;
use Lattice\Stack\Layer;
use Lattice\Stack\LayerOutput;
use Lattice\Stack\StackBuilder;

it("implements Promotion interface", function () {
    $promotion = new Direct(
        reference: 123,
        qualification: Qualification::matchAll(),
        discount: Simple::amountOff(new Money(123, "GBP")),
        budget: Budget::unlimited(),
    );

    expect($promotion)->toBeInstanceOf(PromotionInterface::class);
});

it("can be instantiated", function () {
    $promotion = new Direct(
        reference: 123,
        qualification: Qualification::matchAll(),
        discount: Simple::amountOff(new Money(123, "GBP")),
        budget: Budget::unlimited(),
    );

    expect($promotion->reference)->toBe(123);
    expect($promotion->discount)->toBeInstanceOf(Simple::class);
    expect($promotion->discount->amount)->toEqual(new Money(1_23, "GBP"));
    expect($promotion->budget->redemptionLimit)->toBeNull();
    expect($promotion->budget->monetaryLimit)->toBeNull();
});

it("applies discount correctly", function () {
    $item = Item::fromProduct(
        reference: "item",
        product: new Product(
            reference: "product",
            name: "Product",
            price: new Money(3_00, "GBP"),
            tags: [],
        ),
    );

    $promotion = new Direct(
        reference: "promotion",
        qualification: Qualification::matchAll(),
        discount: Simple::percentageOff(Percentage::fromDecimal(0.1)),
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

    $receipt = $stack->build()->process([$item]);

    expect($receipt->subtotal)->toEqual(new Money(3_00, "GBP"));
    expect($receipt->total)->toEqual(new Money(2_70, "GBP"));
});
