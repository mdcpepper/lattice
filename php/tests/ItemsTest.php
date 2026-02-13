<?php

declare(strict_types=1);

use Lattice\Item;
use Lattice\Money;
use Lattice\Product;

it(
    "creates an item that points to the same product instance",
    function (): void {
        assertLatticeExtensionLoaded();

        $product = new Product(1, "Test Product", new Money(123, "GBP"), [
            "test-tag",
        ]);

        $item = new Item(2, "Test Item", new Money(123, "GBP"), $product);

        expect($item)->toBeInstanceOf(Item::class);
        expect($item->reference)->toBe(2);
        expect($item->name)->toBe("Test Item");
        expect($item->price)->toEqual(new Money(123, "GBP"));
        expect($item->tags)->toBe([]); // doesn't inherit tags automatically
        expect($item->product)->toBeInstanceOf(Product::class);
        expect(spl_object_id($item->product))->toBe(spl_object_id($product));
    },
);

it("builds an item from product", function (): void {
    assertLatticeExtensionLoaded();

    $productReference = (object) ["sku" => "ABC-123"];
    $itemReference = ["external_item_id" => 99];

    $product = new Product(
        $productReference,
        "Test Product",
        new Money(123, "GBP"),
        ["test-tag"],
    );

    $item = Item::fromProduct($itemReference, $product);

    expect($item->reference)->toBe($itemReference);
    expect($item->name)->toBe("Test Product");
    expect($item->price)->toEqual(new Money(123, "GBP"));
    expect($item->tags)->toBe(["test-tag"]);
    expect($item->product)->toBeInstanceOf(Product::class);
    expect($item->product->reference)->toBe($productReference);
});

it("removes duplicate item tags", function (): void {
    assertLatticeExtensionLoaded();

    $product = new Product(1, "Test Product", new Money(123, "GBP"));

    $item = new Item(1, "Test Item", new Money(123, "GBP"), $product, [
        "test-tag",
        "test-tag",
    ]);

    expect($item->tags)->toBe(["test-tag"]);
});
