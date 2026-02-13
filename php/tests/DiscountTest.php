<?php

declare(strict_types=1);

use Lattice\Discount\DiscountKind;
use Lattice\Discount\InvalidPercentageException;
use Lattice\Discount\Percentage;
use Lattice\Discount\PercentageOutOfRangeException;
use Lattice\Discount\SimpleDiscount;
use Lattice\Money;

it("can create percentage from decimal value", function (): void {
    $percentage = Percentage::fromDecimal(0.25);

    expect($percentage->value)->toBe(0.25);
});

it("can create percentage from string representation", function (): void {
    $percentage = new Percentage("0.25");

    expect($percentage->value)->toBe(0.25);
});

it(
    "can create percentage from percentage string with % symbol",
    function (): void {
        $percentage = new Percentage("25%");

        expect($percentage->value)->toBe(0.25);
    },
);

it(
    "throws InvalidPercentageException for invalid percentage string",
    function (): void {
        new Percentage("invalid");
    },
)->throws(
    InvalidPercentageException::class,
    "Invalid percentage value: 'invalid'",
);

it(
    "throws InvalidPercentageException for non-finite percentage decimal",
    function (): void {
        Percentage::fromDecimal(INF);
    },
)->throws(InvalidPercentageException::class, "Percentage value must be finite");

it(
    "throws PercentageOutOfRangeException for negative percentage from string",
    function (): void {
        new Percentage("-10%");
    },
)->throws(
    PercentageOutOfRangeException::class,
    "Discount percentage cannot be negative",
);

it(
    "throws PercentageOutOfRangeException for negative percentage from decimal",
    function (): void {
        Percentage::fromDecimal(-0.1);
    },
)->throws(
    PercentageOutOfRangeException::class,
    "Discount percentage cannot be negative",
);

it(
    "throws PercentageOutOfRangeException for percentage over 100% from string",
    function (): void {
        new Percentage("150%");
    },
)->throws(
    PercentageOutOfRangeException::class,
    "Discount percentage cannot exceed 100%",
);

it(
    "throws PercentageOutOfRangeException for whole number without percent symbol",
    function (): void {
        new Percentage("25");
    },
)->throws(
    PercentageOutOfRangeException::class,
    "Discount percentage cannot exceed 100%",
);

it(
    "throws PercentageOutOfRangeException for percentage over 100% from decimal",
    function (): void {
        Percentage::fromDecimal(1.5);
    },
)->throws(
    PercentageOutOfRangeException::class,
    "Discount percentage cannot exceed 100%",
);

it("accepts 0% discount", function (): void {
    $percentage = new Percentage("0%");

    expect($percentage->value)->toBe(0.0);
});

it("accepts 100% discount", function (): void {
    $percentage = new Percentage("100%");

    expect($percentage->value)->toBe(1.0);
});

it("can create percentage off discount", function (): void {
    $percentage = new Percentage("0.25");
    $discount = SimpleDiscount::percentageOff($percentage);

    expect($discount->kind)->toBe(DiscountKind::PercentageOff);
    expect($discount->percentage)->not->toBeNull();
    expect($discount->amount)->toBeNull();
});

it("can create amount override discount", function (): void {
    $amount = new Money(500, "GBP");
    $discount = SimpleDiscount::amountOverride($amount);

    expect($discount->kind)->toBe(DiscountKind::AmountOverride);
    expect($discount->percentage)->toBeNull();
    expect($discount->amount)->not->toBeNull();
});

it("can create amount off discount", function (): void {
    $amount = new Money(200, "GBP");
    $discount = SimpleDiscount::amountOff($amount);

    expect($discount->kind)->toBe(DiscountKind::AmountOff);
    expect($discount->percentage)->toBeNull();
    expect($discount->amount)->not->toBeNull();
});

it("percentage off discount uses correct discount kind", function (): void {
    $percentage = Percentage::fromDecimal(0.5);
    $discount = SimpleDiscount::percentageOff($percentage);

    expect($discount->kind)->toBe(DiscountKind::PercentageOff);
});

it("amount override discount uses correct discount kind", function (): void {
    $amount = new Money(1000, "USD");
    $discount = SimpleDiscount::amountOverride($amount);

    expect($discount->kind)->toBe(DiscountKind::AmountOverride);
});

it("amount off discount uses correct discount kind", function (): void {
    $amount = new Money(300, "EUR");
    $discount = SimpleDiscount::amountOff($amount);

    expect($discount->kind)->toBe(DiscountKind::AmountOff);
});
