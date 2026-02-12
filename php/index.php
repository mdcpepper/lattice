<?php

declare(strict_types=1);

use FeedCode\Lattice\Engine;
use FeedCode\Lattice\Item;
use FeedCode\Lattice\Layer;
use FeedCode\Lattice\LayerOutput;
use FeedCode\Lattice\Money;
use FeedCode\Lattice\Promotions\MixAndMatchFixedTotalDiscount;
use FeedCode\Lattice\Promotions\MixAndMatchPromotion;
use FeedCode\Lattice\Promotions\MixAndMatchSlot;
use FeedCode\Lattice\Product;
use FeedCode\Lattice\PromotionBudget;
use FeedCode\Lattice\Qualification;
use FeedCode\Lattice\StackBuilder;

$sandwich = new Product(
    key: 1,
    name: "Sandwich",
    price: new Money(325, "GBP"),
    tags: ["meal-deal:main"],
);

$snack = new Product(
    key: 2,
    name: "Crisps",
    price: new Money(125, "GBP"),
    tags: ["meal-deal:side"],
);

$drink = new Product(
    key: 3,
    name: "Orange Juice",
    price: new Money(175, "GBP"),
    tags: ["meal-deal:drink"],
);

$sandwichItem = Item::from_product(11, $sandwich);
$snackItem = Item::from_product(12, $snack);
$drinkItem = Item::from_product(13, $drink);

$mealDeal = new MixAndMatchPromotion(
    key: "meal-deal",
    slots: [
        new MixAndMatchSlot(
            key: "main",
            qualification: Qualification::matchAny(["meal-deal:main"]),
            min: 1,
            max: 1,
        ),
        new MixAndMatchSlot(
            key: "side",
            qualification: Qualification::matchAny(["meal-deal:side"]),
            min: 1,
            max: 1,
        ),
        new MixAndMatchSlot(
            key: "drink",
            qualification: Qualification::matchAny(["meal-deal:drink"]),
            min: 1,
            max: 1,
        ),
    ],
    discount: new MixAndMatchFixedTotalDiscount(amount: new Money(350, "GBP")),
    budget: PromotionBudget::unlimited(),
);

$stackBuilder = new StackBuilder();

$mealDealLayer = $stackBuilder->add_layer(
    new Layer(
        key: "meal-deal",
        output: LayerOutput::pass_through(),
        promotions: [$mealDeal],
    ),
);

$stackBuilder->set_root($mealDealLayer);

$stack = $stackBuilder->build();

$receipt = Engine::process(
    items: [$sandwichItem, $snackItem, $drinkItem],
    stack: $stack,
);
