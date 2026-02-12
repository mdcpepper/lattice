<?php
declare(strict_types=1);

namespace FeedCode\Lattice;

if (!class_exists(Money::class)) {
    class Money
    {
        public int $amount;
        public string $currency;

        public function __construct(int $amount, string $currency) {}
    }
}

if (!class_exists(Product::class)) {
    class Product
    {
        public mixed $reference;
        public string $name;
        public int $price;
        /** @var string[] */
        public array $tags;

        /**
         * @param string[]|null $tags
         */
        public function __construct(
            mixed $reference,
            string $name,
            int $price,
            ?array $tags = [],
        ) {}
    }
}

if (!class_exists(Item::class)) {
    class Item
    {
        public mixed $id;
        public string $name;
        public int $price;
        public Product $product;

        /** @var string[] */
        public array $tags;

        /**
         * @param string[]|null $tags
         */
        public function __construct(
            mixed $id,
            string $name,
            int $price,
            Product $product,
            ?array $tags = [],
        ) {}

        public static function from_product(
            mixed $reference,
            Product $product,
        ): self {}
    }
}
