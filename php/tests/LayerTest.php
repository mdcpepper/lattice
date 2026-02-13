<?php

declare(strict_types=1);

use FeedCode\Lattice\Discount\SimpleDiscount;
use FeedCode\Lattice\Layer;
use FeedCode\Lattice\LayerOutput;
use FeedCode\Lattice\Money;
use FeedCode\Lattice\Promotions\Budget;
use FeedCode\Lattice\Promotions\DirectDiscount;
use FeedCode\Lattice\Qualification;

it("supports layer output enum values", function (): void {
    assertLatticeExtensionLoaded();

    expect(LayerOutput::PassThrough->value)->toBe("pass_through");
    expect(LayerOutput::Split->value)->toBe("split");
});

it("can build a layer with direct discount promotions", function (): void {
    assertLatticeExtensionLoaded();

    $promotion = new DirectDiscount(
        key: "meal-deal",
        qualification: Qualification::matchAny(["meal-deal:main"]),
        discount: SimpleDiscount::amountOff(new Money(50, "GBP")),
        budget: Budget::unlimited(),
    );

    $layer = new Layer(
        key: "meal-deal",
        output: LayerOutput::PassThrough,
        promotions: [$promotion],
    );

    expect($layer->key)->toBe("meal-deal");
    expect($layer->output)->toBe(LayerOutput::PassThrough);
    expect($layer->promotions)->toHaveCount(1);
    expect($layer->promotions[0])->toBeInstanceOf(DirectDiscount::class);
});

it("defaults a layer to an empty promotions list", function (): void {
    assertLatticeExtensionLoaded();

    $layer = new Layer(key: "empty", output: LayerOutput::PassThrough);

    expect($layer->promotions)->toBe([]);
});
