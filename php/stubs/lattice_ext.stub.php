<?php
declare(strict_types=1);

namespace Lattice;

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
        public Money $price;

        /** @var string[] */
        public array $tags;

        /**
         * @param string[]|null $tags
         */
        public function __construct(
            mixed $reference,
            string $name,
            Money $price,
            ?array $tags = [],
        ) {}
    }
}

if (!class_exists(Item::class)) {
    class Item
    {
        public mixed $reference;
        public string $name;
        public Money $price;
        public Product $product;

        /** @var string[] */
        public array $tags;

        /**
         * @param string[]|null $tags
         */
        public function __construct(
            mixed $reference,
            string $name,
            Money $price,
            Product $product,
            ?array $tags = [],
        ) {}

        public static function fromProduct(
            mixed $reference,
            Product $product,
        ): self {}
    }
}

if (!enum_exists(LayerOutput::class)) {
    enum LayerOutput: string
    {
        case PassThrough = "pass_through";
        case Split = "split";

        public static function passThrough(): self {}
        public static function split(): self {}
    }
}

if (!class_exists(Layer::class)) {
    class Layer
    {
        public mixed $reference;
        public LayerOutput $output;

        /** @var Lattice\Promotions\Promotion[] */
        public array $promotions;

        /**
         * @param Lattice\Promotions\Promotion[]|null $promotions
         */
        public function __construct(
            mixed $reference,
            LayerOutput $output,
            array $promotions,
        ) {}
    }
}

if (!class_exists(Stack::class)) {
    class Stack
    {
        /** @var Layer[] */
        public array $layers;

        /**
         * @param Layer[] $layers
         */
        public function __construct(array $layers = []) {}

        public function validateGraph(): bool {}

        /**
         * @param Item[] $items
         */
        public function process(array $items): Receipt {}
    }
}

if (!class_exists(StackBuilder::class)) {
    class StackBuilder
    {
        /** @var Layer[] */
        public array $layers;

        public ?Layer $rootLayer;

        public function __construct() {}

        public function addLayer(Layer $layer): Layer {}

        public function setRoot(Layer $layer): void {}

        public function build(): Stack {}
    }
}

if (!class_exists(PromotionApplication::class)) {
    class PromotionApplication
    {
        public Lattice\Promotions\Promotion $promotion;
        public Item $item;
        public int $bundleId;
        public Money $originalPrice;
        public Money $finalPrice;

        public function __construct(
            Lattice\Promotions\Promotion $promotion,
            Item $item,
            int $bundleId,
            Money $originalPrice,
            Money $finalPrice,
        ) {}
    }
}

if (!class_exists(Receipt::class)) {
    class Receipt
    {
        public Money $subtotal;
        public Money $total;

        /** @var Item[] */
        public array $fullPriceItems;

        /** @var PromotionApplication[] */
        public array $promotionApplications;

        /**
         * @param Item[]|null $fullPriceItems
         * @param PromotionApplication[]|null $promotionApplications
         */
        public function __construct(
            Money $subtotal,
            Money $total,
            ?array $fullPriceItems = [],
            ?array $promotionApplications = [],
        ) {}
    }
}

namespace Lattice\Stack;

if (!class_exists(InvalidStackException::class)) {
    class InvalidStackException extends \Exception {}
}

namespace Lattice;

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

namespace Lattice\Qualification;

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

        public ?Lattice\Qualification $group;

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
            Lattice\Qualification $qualification,
        ): self {}

        /**
         * @param string[]|null $item_tags
         */
        public function matches(?array $item_tags = []): bool {}
    }
}

namespace Lattice\Discount;

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
        public ?Money $amount;

        public static function percentageOff(Percentage $percentage): self {}

        public static function amountOverride(Money $amount): self {}

        public static function amountOff(Money $amount): self {}
    }
}

namespace Lattice\Promotions;

use Lattice\Discount\SimpleDiscount;
use Lattice\Qualification;

if (!class_exists(Budget::class)) {
    class Budget
    {
        public ?int $applicationLimit;
        public ?Money $monetaryLimit;

        public static function unlimited(): self {}

        public static function withApplicationLimit(int $limit): self {}

        public static function withMonetaryLimit(Money $limit): self {}

        public static function withBothLimits(
            int $monetaryLimit,
            Money $limit,
        ): self {}
    }
}

if (!interface_exists(Promotion::class)) {
    interface Promotion {}
}

if (!class_exists(DirectDiscount::class)) {
    class DirectDiscount implements Promotion
    {
        public mixed $reference;
        public Qualification $qualification;
        public SimpleDiscount $discount;
        public Budget $budget;

        public function __construct(
            mixed $reference,
            Qualification $qualification,
            SimpleDiscount $discount,
            Budget $budget,
        ) {}
    }
}
