<?php

declare(strict_types=1);

use Lattice\Money;
use Lattice\Product;

it("creates a product with expected properties", function (): void {
    $product = new Product(1, "Test Product", new Money(123, "GBP"), [
        "test-tag",
    ]);

    expect($product)->toBeInstanceOf(Product::class);
    expect($product->reference)->toBe(1);
    expect($product->name)->toBe("Test Product");
    expect($product->tags)->toBe(["test-tag"]);
    expect($product->price)->toEqual(new Money(123, "GBP"));
});

it("removes duplicate product tags", function (): void {
    $product = new Product(1, "Test Product", new Money(123, "GBP"), [
        "test-tag",
        "test-tag",
    ]);

    expect($product->tags)->toBe(["test-tag"]);
});
