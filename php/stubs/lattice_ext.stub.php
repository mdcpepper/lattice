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
        public mixed $key;
        public string $name;
        public \FeedCode\Lattice\Money $price;

        /** @var string[] */
        public array $tags;

        /**
         * @param string[]|null $tags
         */
        public function __construct(
            mixed $key,
            string $name,
            \FeedCode\Lattice\Money $price,
            ?array $tags = [],
        ) {}
    }
}

if (!class_exists(Item::class)) {
    class Item
    {
        public mixed $key;
        public string $name;
        public \FeedCode\Lattice\Money $price;
        public Product $product;

        /** @var string[] */
        public array $tags;

        /**
         * @param string[]|null $tags
         */
        public function __construct(
            mixed $key,
            string $name,
            \FeedCode\Lattice\Money $price,
            Product $product,
            ?array $tags = [],
        ) {}

        public static function fromProduct(
            mixed $key,
            Product $product,
        ): self {}
    }
}

if (!enum_exists(LayerOutput::class)) {
    enum LayerOutput: string
    {
        case PassThrough = "pass_through";
        case Split = "split";
    }
}

if (!class_exists(Layer::class)) {
    class Layer
    {
        public mixed $key;
        public LayerOutput $output;

        /** @var \FeedCode\Lattice\Promotions\Promotion[] */
        public array $promotions;

        /**
         * @param \FeedCode\Lattice\Promotions\Promotion[]|null $promotions
         */
        public function __construct(
            mixed $key,
            LayerOutput $output,
            ?array $promotions = [],
        ) {}
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

namespace FeedCode\Lattice\Discount;

if (!class_exists(InvalidPercentageException::class)) {
    class InvalidPercentageException extends \Exception {}
}

if (!class_exists(PercentageOutOfRangeException::class)) {
    class PercentageOutOfRangeException extends \Exception {}
}

if (!class_exists(InvalidDiscountException::class)) {
    class InvalidDiscountException extends \Exception {}
}

if (!class_exists(Percentage::class)) {
    class Percentage
    {
        public readonly float $value;

        public function __construct(string $value) {}

        public static function fromDecimal(float $value): self {}

        public function value(): float {}
    }
}

if (!enum_exists(DiscountKind::class)) {
    enum DiscountKind: string
    {
        case PercentageOff = "percentage_off";
        case AmountOverride = "amount_override";
        case AmountOff = "amount_off";
    }
}

if (!class_exists(SimpleDiscount::class)) {
    class SimpleDiscount
    {
        public DiscountKind $kind;
        public ?Percentage $percentage;
        public ?\FeedCode\Lattice\Money $amount;

        public static function percentageOff(Percentage $percentage): self {}

        public static function amountOverride(
            \FeedCode\Lattice\Money $amount,
        ): self {}

        public static function amountOff(
            \FeedCode\Lattice\Money $amount,
        ): self {}
    }
}

namespace FeedCode\Lattice\Promotions;

use FeedCode\Lattice\Discount\SimpleDiscount;
use FeedCode\Lattice\Qualification;

if (!class_exists(Budget::class)) {
    class Budget
    {
        public ?int $applicationLimit;
        public ?\FeedCode\Lattice\Money $monetaryLimit;

        public static function unlimited(): self {}

        public static function withApplicationLimit(int $limit): self {}

        public static function withMonetaryLimit(
            \FeedCode\Lattice\Money $limit,
        ): self {}

        public static function withBothLimits(
            int $monetaryLimit,
            \FeedCode\Lattice\Money $limit,
        ): self {}
    }
}

if (!interface_exists(Promotion::class)) {
    interface Promotion {}
}

if (!class_exists(DirectDiscount::class)) {
    class DirectDiscount implements Promotion
    {
        public mixed $key;
        public Qualification $qualification;
        public SimpleDiscount $discount;
        public Budget $budget;

        public function __construct(
            mixed $key,
            Qualification $qualification,
            SimpleDiscount $discount,
            Budget $budget,
        ) {}
    }
}
