<?php

declare(strict_types=1);

use Lattice\Qualification;
use Lattice\Qualification\BoolOp;
use Lattice\Qualification\Rule;

it("empty qualification matches all", function (): void {
    $qualification = Qualification::matchAll();

    expect($qualification->matches(["peak", "snack"]))->toBeTrue();
});

it("matches hasAny tags", function (): void {
    $qualification = new Qualification(BoolOp::AndOp, [
        Rule::hasAny(["member", "staff"]),
    ]);

    expect($qualification->matches(["peak", "member"]))->toBeTrue();
    expect($qualification->matches(["peak", "staff"]))->toBeTrue();
    expect($qualification->matches(["peak", "snack"]))->toBeFalse();
});

it("matches hasAll tags", function (): void {
    $qualification = new Qualification(BoolOp::AndOp, [
        Rule::hasAll(["peak", "snack"]),
    ]);

    expect($qualification->matches(["peak", "snack", "member"]))->toBeTrue();
    expect($qualification->matches(["peak"]))->toBeFalse();
    expect($qualification->matches(["snack"]))->toBeFalse();
});

it("matches hasNone tags", function (): void {
    $qualification = new Qualification(BoolOp::AndOp, [
        Rule::hasNone(["excluded", "blocked"]),
    ]);

    expect($qualification->matches(["peak", "snack"]))->toBeTrue();
    expect($qualification->matches(["peak", "excluded"]))->toBeFalse();
    expect($qualification->matches(["snack", "blocked"]))->toBeFalse();
});

it("supports nested boolean groups", function (): void {
    $qualification = new Qualification(BoolOp::AndOp, [
        Rule::hasAll(["peak", "snack"]),
        Rule::group(
            new Qualification(BoolOp::OrOp, [
                Rule::hasAny(["member", "staff"]),
                Rule::hasNone(["excluded"]),
            ]),
        ),
    ]);

    expect($qualification->matches(["peak", "snack", "member"]))->toBeTrue();
    expect($qualification->matches(["peak", "snack"]))->toBeTrue();
    expect($qualification->matches(["peak", "member"]))->toBeFalse();
    expect($qualification->matches(["peak", "snack", "excluded"]))->toBeFalse();
});
