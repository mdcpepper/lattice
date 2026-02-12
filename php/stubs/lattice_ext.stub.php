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

        public static function fromProduct(
            mixed $reference,
            Product $product,
        ): self {}
    }
}

if (!class_exists(Qualification::class)) {
    class Qualification
    {
        public Qualification\BoolOp $op;

        /** @var Qualification\Rule[] */
        public array $rules;

        /**
         * @param Qualification\Rule[]|null $rules
         */
        public function __construct(
            Qualification\BoolOp $op,
            ?array $rules = [],
        ) {}

        public static function matchAll(): self {}

        /**
         * @param string[]|null $tags
         */
        public static function matchAny(?array $tags = []): self {}

        /**
         * @param string[]|null $item_tags
         */
        public function matches(?array $item_tags = []): bool {}
    }
}

namespace FeedCode\Lattice\Qualification;

if (!class_exists(BoolOp::class)) {
    enum BoolOp: string
    {
        case AndOp = "and";
        case OrOp = "or";
    }
}

if (!class_exists(RuleKind::class)) {
    enum RuleKind: string
    {
        case HasAll = "has_all";
        case HasAny = "has_any";
        case HasNone = "has_none";
        case Group = "group";
    }
}

if (!class_exists(Rule::class)) {
    class Rule
    {
        public RuleKind $kind;

        /** @var string[] */
        public array $tags;

        public ?\FeedCode\Lattice\Qualification $group;

        /**
         * @param string[]|null $tags
         */
        public static function hasAll(?array $tags = []): self {}

        /**
         * @param string[]|null $tags
         */
        public static function hasAny(?array $tags = []): self {}

        /**
         * @param string[]|null $tags
         */
        public static function hasNone(?array $tags = []): self {}

        public static function group(
            \FeedCode\Lattice\Qualification $qualification,
        ): self {}

        /**
         * @param string[]|null $item_tags
         */
        public function matches(?array $item_tags = []): bool {}
    }
}
