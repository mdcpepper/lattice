<?php

declare(strict_types=1);

use Lattice\Discount\SimpleDiscount;
use Lattice\Layer;
use Lattice\LayerOutput;
use Lattice\Money;
use Lattice\Promotions\Budget;
use Lattice\Promotions\DirectDiscount;
use Lattice\Qualification;

it("supports layer output enum values", function (): void {
    assertLatticeExtensionLoaded();

    expect(LayerOutput::PassThrough->value)->toBe("pass_through");
    expect(LayerOutput::Split->value)->toBe("split");
});

it("can build a layer with direct discount promotions", function (): void {
    assertLatticeExtensionLoaded();

    $promotion = new DirectDiscount(
        reference: "meal-deal",
        qualification: Qualification::matchAny(["meal-deal:main"]),
        discount: SimpleDiscount::amountOff(new Money(50, "GBP")),
        budget: Budget::unlimited(),
    );

    $layer = new Layer(
        reference: "meal-deal",
        output: LayerOutput::PassThrough,
        promotions: [$promotion],
    );

    expect($layer->reference)->toBe("meal-deal");
    expect($layer->output)->toBe(LayerOutput::PassThrough);
    expect($layer->promotions)->toHaveCount(1);
    expect($layer->promotions[0])->toBeInstanceOf(DirectDiscount::class);
});
