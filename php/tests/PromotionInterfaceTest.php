<?php

declare(strict_types=1);

it("registers the promotions marker interface", function (): void {
    expect(interface_exists("Lattice\\Promotions\\Promotion"))->toBeTrue();
});
