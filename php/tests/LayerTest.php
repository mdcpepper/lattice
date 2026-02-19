<?php

declare(strict_types=1);

use Lattice\Discount\Simple;
use Lattice\Money;
use Lattice\Promotion\Budget;
use Lattice\Promotion\Direct;
use Lattice\Qualification;
use Lattice\Stack\Layer;
use Lattice\Stack\LayerOutput;

it("supports layer output factory methods", function (): void {
    $participating = new Layer(
        reference: "participating",
        output: LayerOutput::passThrough(),
        promotions: [],
    );

    $nonParticipating = new Layer(
        reference: "non-participating",
        output: LayerOutput::passThrough(),
        promotions: [],
    );

    $passThrough = LayerOutput::passThrough();
    $split = LayerOutput::split(
        participating: $participating,
        nonParticipating: $nonParticipating,
    );

    expect($passThrough)->toBeInstanceOf(LayerOutput::class);
    expect($split)->toBeInstanceOf(LayerOutput::class);
});

it("can build a layer with direct discount promotions", function (): void {
    $promotion = new Direct(
        reference: "direct-discount",
        qualification: Qualification::matchAny(["direct-discount"]),
        discount: Simple::amountOff(new Money(50, "GBP")),
        budget: Budget::unlimited(),
    );

    $layer = new Layer(
        reference: "direct-discount",
        output: LayerOutput::passThrough(),
        promotions: [$promotion],
    );

    expect($layer->reference)->toBe("direct-discount");
    expect($layer->output)->toBeInstanceOf(LayerOutput::class);
    expect($layer->promotions)->toHaveCount(1);
    expect($layer->promotions[0])->toBeInstanceOf(Direct::class);
});
