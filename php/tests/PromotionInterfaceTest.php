<?php

declare(strict_types=1);

it("registers the promotions marker interface", function (): void {
    assertLatticeExtensionLoaded();

    expect(interface_exists("FeedCode\\Lattice\\Promotions\\Promotion"))->toBeTrue();
});
