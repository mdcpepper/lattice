<?php

declare(strict_types=1);

use Lattice\Discount\Simple;
use Lattice\Discount\Percentage;
use Lattice\Item;
use Lattice\Money;
use Lattice\Product;
use Lattice\PromotionRedemption;
use Lattice\Promotion\Budget;
use Lattice\Promotion\Direct;
use Lattice\Qualification;
use Lattice\Receipt;
use Lattice\Stack\InvalidStackException;
use Lattice\Stack\Layer;
use Lattice\Stack\LayerOutput;
use Lattice\Stack\Stack;
use Lattice\Stack\StackBuilder;

it("validates a linear stack as a promotion graph", function (): void {
    $promotion = new Direct(
        reference: "promo-1",
        qualification: Qualification::matchAny(["food"]),
        discount: Simple::amountOff(new Money(25, "GBP")),
        budget: Budget::unlimited(),
    );

    $stack = new Stack([
        new Layer(
            reference: "layer-1",
            output: LayerOutput::passThrough(),
            promotions: [$promotion],
        ),
    ]);

    expect($stack->validateGraph())->toBeTrue();
});

it("throws when validating an empty stack", function (): void {
    $stack = new Stack();

    $thrown = null;

    try {
        $stack->validateGraph();
    } catch (Throwable $error) {
        $thrown = $error;
    }

    expect($thrown)->toBeInstanceOf(InvalidStackException::class);
    expect($thrown?->getMessage())->toContain("at least one layer");
});

it(
    "throws when split output references layers outside the stack",
    function (): void {
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

        $stack = new Stack([
            new Layer(
                reference: "split-layer",
                output: LayerOutput::split(
                    participating: $participating,
                    nonParticipating: $nonParticipating,
                ),
                promotions: [],
            ),
        ]);

        $thrown = null;

        try {
            $stack->validateGraph();
        } catch (Throwable $error) {
            $thrown = $error;
        }

        expect($thrown)->toBeInstanceOf(InvalidStackException::class);
        expect($thrown?->getMessage())->toContain(
            "split output participating target must be one of",
        );
    },
);

it("builds and processes a single-layer stack", function (): void {
    $item = Item::fromProduct(
        reference: "item",
        product: new Product(
            reference: "product",
            name: "Sandwich",
            price: new Money(3_00, "GBP"),
            tags: ["eligible"],
        ),
    );

    $tenPercentOff = new Direct(
        reference: "ten-off",
        qualification: Qualification::matchAny(["eligible"]),
        discount: Simple::percentageOff(Percentage::fromDecimal(0.1)),
        budget: Budget::unlimited(),
    );

    $stack = new StackBuilder();

    $layer = $stack->addLayer(
        new Layer(
            reference: "layer-one",
            output: LayerOutput::passThrough(),
            promotions: [$tenPercentOff],
        ),
    );

    $stack->setRoot($layer);

    $receipt = $stack->build()->process(items: [$item]);

    expect($receipt)->toBeInstanceOf(Receipt::class);
    expect($receipt->subtotal)->toEqual(new Money(3_00, "GBP"));
    expect($receipt->total)->toEqual(new Money(2_70, "GBP"));
});

it(
    "builds and processes a two-layer stack and applies only the best layer-two discount",
    function (): void {
        $sandwich = new Product(
            reference: "p-main",
            name: "Sandwich",
            price: new Money(100_00, "GBP"),
            tags: ["eligible"],
        );

        $snack = new Product(
            reference: "p-side",
            name: "Crisps",
            price: new Money(1_01, "GBP"),
            tags: ["other"],
        );

        $sandwichItem = Item::fromProduct("i-main", $sandwich);
        $snackItem = Item::fromProduct("i-side", $snack);

        $elevenOff = new Direct(
            reference: "eleven-off",
            qualification: Qualification::matchAny(["eligible"]),
            discount: Simple::percentageOff(
                Percentage::fromDecimal(0.11),
            ),
            budget: Budget::unlimited(),
        );

        $thirteenOff = new Direct(
            reference: "thirteen-off",
            qualification: Qualification::matchAny(["eligible"]),
            discount: Simple::percentageOff(
                Percentage::fromDecimal(0.13),
            ),
            budget: Budget::unlimited(),
        );

        $seventeenOff = new Direct(
            reference: "seventeen-off",
            qualification: Qualification::matchAny(["eligible"]),
            discount: Simple::percentageOff(
                Percentage::fromDecimal(0.17),
            ),
            budget: Budget::unlimited(),
        );

        $stackBuilder = new StackBuilder();

        $layerOne = $stackBuilder->addLayer(
            new Layer(
                reference: "layer-one",
                output: LayerOutput::passThrough(),
                promotions: [$elevenOff],
            ),
        );

        $stackBuilder->addLayer(
            new Layer(
                reference: "layer-two",
                output: LayerOutput::passThrough(),
                promotions: [$thirteenOff, $seventeenOff],
            ),
        );

        $stackBuilder->setRoot($layerOne);
        $stack = $stackBuilder->build();

        $receipt = $stack->process(items: [$sandwichItem, $snackItem]);

        expect($receipt)->toBeInstanceOf(Receipt::class);
        expect($receipt->subtotal)->toEqual(new Money(101_01, "GBP"));
        expect($receipt->total)->toEqual(new Money(74_88, "GBP"));

        expect($receipt->fullPriceItems)->toHaveCount(1);
        expect($receipt->fullPriceItems[0])->toBe($snackItem);

        expect($receipt->promotionRedemptions)->toHaveCount(2);

        /** @var PromotionRedemption $firstRedemption */
        $firstRedemption = $receipt->promotionRedemptions[0];

        /** @var PromotionRedemption $secondRedemption */
        $secondRedemption = $receipt->promotionRedemptions[1];

        expect($firstRedemption)->toBeInstanceOf(PromotionRedemption::class);
        expect($firstRedemption->promotion)->toBe($elevenOff);
        expect($firstRedemption->item)->toBe($sandwichItem);
        expect($firstRedemption->originalPrice)->toEqual(
            new Money(100_00, "GBP"),
        );
        expect($firstRedemption->finalPrice)->toEqual(new Money(89_00, "GBP"));

        expect($secondRedemption)->toBeInstanceOf(PromotionRedemption::class);
        expect($secondRedemption->promotion)->toBe($seventeenOff);
        expect($secondRedemption->item)->toBe($sandwichItem);
        expect($secondRedemption->originalPrice)->toEqual(
            new Money(89_00, "GBP"),
        );
        expect($secondRedemption->finalPrice)->toEqual(new Money(7387, "GBP"));
    },
);

it(
    "routes non-participating items to a staff-discount layer while participating items skip it",
    function (): void {
        $sandwich = Item::fromProduct(
            "i-main",
            new Product(
                reference: "p-main",
                name: "Sandwich",
                price: new Money(100_00, "GBP"),
                tags: ["eligible", "staff"],
            ),
        );

        $snack = Item::fromProduct(
            "i-side",
            new Product(
                reference: "p-side",
                name: "Crisps",
                price: new Money(10_00, "GBP"),
                tags: [],
            ),
        );

        $tenOffEligible = new Direct(
            reference: "ten-off-eligible",
            qualification: Qualification::matchAny(["eligible"]),
            discount: Simple::percentageOff(
                Percentage::fromDecimal(0.1),
            ),
            budget: Budget::unlimited(),
        );

        $staffDiscount = new Direct(
            reference: "five-off-staff",
            qualification: Qualification::matchAll(),
            discount: Simple::percentageOff(
                Percentage::fromDecimal(0.05),
            ),
            budget: Budget::unlimited(),
        );

        $rootLayer = new Layer(
            reference: "ten-off",
            output: LayerOutput::split(
                participating: ($participatingLayer = new Layer(
                    reference: "participating-passthrough",
                    output: LayerOutput::passThrough(),
                    promotions: [],
                )),
                nonParticipating: ($staffLayer = new Layer(
                    reference: "staff-discount",
                    output: LayerOutput::passThrough(),
                    promotions: [$staffDiscount],
                )),
            ),
            promotions: [$tenOffEligible],
        );

        $stackBuilder = new StackBuilder();

        $root = $stackBuilder->addLayer($rootLayer);
        $stackBuilder->addLayer($participatingLayer);
        $stackBuilder->addLayer($staffLayer);
        $stackBuilder->setRoot($root);

        $receipt = $stackBuilder->build()->process(items: [$sandwich, $snack]);

        expect($receipt)->toBeInstanceOf(Receipt::class);
        expect($receipt->subtotal)->toEqual(new Money(110_00, "GBP"));
        expect($receipt->total)->toEqual(new Money(99_50, "GBP"));
        expect($receipt->fullPriceItems)->toHaveCount(0);
        expect($receipt->promotionRedemptions)->toHaveCount(2);

        /** @var PromotionRedemption $firstRedemption */
        $firstRedemption = $receipt->promotionRedemptions[0];

        /** @var PromotionRedemption $secondRedemption */
        $secondRedemption = $receipt->promotionRedemptions[1];

        expect($firstRedemption->promotion)->toBe($tenOffEligible);
        expect($firstRedemption->item)->toBe($sandwich);
        expect($firstRedemption->originalPrice)->toEqual(
            new Money(100_00, "GBP"),
        );
        expect($firstRedemption->finalPrice)->toEqual(new Money(90_00, "GBP"));

        expect($secondRedemption->promotion)->toBe($staffDiscount);
        expect($secondRedemption->item)->toBe($snack);
        expect($secondRedemption->originalPrice)->toEqual(
            new Money(10_00, "GBP"),
        );
        expect($secondRedemption->finalPrice)->toEqual(new Money(9_50, "GBP"));
    },
);
