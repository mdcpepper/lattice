<?php

declare(strict_types=1);

pest()->beforeEach(function () {
    assertLatticeExtensionLoaded();
});

function assertLatticeExtensionLoaded(): void
{
    if (extension_loaded("lattice-php-ext")) {
        return;
    }

    throw new RuntimeException(
        "The lattice-php-ext extension is not loaded. Run tests with: " .
            "php -d extension=../target/debug/liblattice.so vendor/bin/pest",
    );
}
