# Project Lattice

[![Build & Test](https://github.com/mdcpepper/lattice/actions/workflows/ci.yml/badge.svg)](https://github.com/mdcpepper/latice/actions/workflows/ci.yml)

Lattice is a high-performance, general-purpose pricing, promotion and basket 
optimisation engine written in Rust.

* [Promotion Types](#promotion-types)
  * [Direct Discount Promotions](#direct-discount-promotions)
  * [Positional Discount Promotions](#positional-discount-promotions)
  * [Mix and Match Promotions](#mix-and-match-promotions)
  * [Tiered Threshold Promotions](#tiered-threshold-promotions)
* [Qualification](#qualification)
* [Budgets](#budgets)
  * [Application Budgets](#application-budgets)
  * [Monetary Budgets](#monetary-budgets)
* [Global Optimisation](#global-optimisation)
* [Stacking](#stacking)
* [Export ILP Formulation](#export-ilp-formulation)

## Promotion Types

Promotions are rules that select candidate items via tag qualifications, and
apply a discount to them. Promotion applications are treated as a global basket
optimisation problem and the combination that produces the lowest total basket 
price is chosen.

Within a layer, each item is either left at full price or claimed by exactly one 
promotion. When using a promotion graph, items can flow through multiple layers 
and accumulate multiple applications (stacking). Promotion types can add their 
own constraints (e.g. bundled deals) and produce per-item applications 
(including original/final prices and bundle groupings) that can be rendered on 
a receipt. Each promotion type is documented in its own section below.

_NOTE: Example timings are from release builds running on an AMD Ryzen 7 5700U (8C/16T @ 4.37 GHz, 28 GB RAM), using the `microlp` solver_.

### Direct Discount Promotions

A simple, direct percentage discount or fixed price override applied to qualifying 
items independently (no bundling).

```yaml
20-off:
  type: direct_discount
  name: 20% Off
  tags: [20-off]
  discount:
    type: percentage_off
    amount: 20%

40-off:
  type: direct_discount
  name: 40% Off
  tags: [40-off]
  discount:
    type: percentage_off
    amount: 40%
```

```bash
cargo run --release --example basket -- -f direct
```

```
╭──────┬──────────┬────────┬────────────┬──────────────────┬─────────────────┬──────────────╮
│      │ Item     │ Tags   │ Base Price │ Discounted Price │         Savings │ Promotion    │
├──────┼──────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────┤
│ #1   │ Sandwich │        │      £2.99 │                  │                 │              │
├──────┼──────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────┤
│ #2   │ Drink    │ 20-off │      £1.29 │            £1.03 │ (20.16%) -£0.26 │ #1   20% Off │
├──────┼──────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────┤
│ #3   │ Snack    │ 20-off │      £0.79 │            £0.47 │ (40.51%) -£0.32 │ #2   40% Off │
│      │          │ 40-off │            │                  │                 │              │
╰──────┴──────────┴────────┴────────────┴──────────────────┴─────────────────┴──────────────╯
 Subtotal:            £5.07  
    Total:            £4.49  
  Savings:   (11.44%) £0.58  

 56µs 270ns (0.00005627s)
```

In this example "Drink" qualifies for the "20% off" promotion, and "Snack" 
qualifies for _both_ the "20% off" and the "40% off" promotion. Applying the 40% 
promotion to the "Snack" item results in the cheapest total basket price, so 
it is that one that is applied to that item.

### Positional Discount Promotions

These promotions apply discounts to specific positions when items are ordered 
by price. This category encompasses BOGOF (2-for-1), BOGOHP (second item half price), 
3-for-2, 5-for-3, and similar X-for-Y offers.

```yaml
3-for-2:
  type: positional_discount
  name: 3-for-2 Vitamins
  tags: [3-for-2]
  size: 3
  positions: [2]
  discount:
    type: percentage_off
    amount: 100%
```

```bash
cargo run --release --example basket -- -f positional
```

```
╭──────┬─────────────────────────────┬─────────┬────────────┬──────────────────┬───────────────┬───────────────────────╮
│      │ Item                        │ Tags    │ Base Price │ Discounted Price │       Savings │ Promotion             │
├──────┼─────────────────────────────┼─────────┼────────────┼──────────────────┼───────────────┼───────────────────────┤
│ #1   │ Multivitamins 30 gummies    │ 3-for-2 │      £4.50 │                  │               │ #1   3-for-2 Vitamins │
├──────┼─────────────────────────────┼─────────┼────────────┼──────────────────┼───────────────┼───────────────────────┤
│ #2   │ Vitamin C 1000mg 20 Tablets │ 3-for-2 │      £1.99 │            £0.00 │ (100%) -£1.99 │ #1   3-for-2 Vitamins │
├──────┼─────────────────────────────┼─────────┼────────────┼──────────────────┼───────────────┼───────────────────────┤
│ #3   │ Magnesium 180 Tablets       │ 3-for-2 │     £12.85 │                  │               │ #1   3-for-2 Vitamins │
╰──────┴─────────────────────────────┴─────────┴────────────┴──────────────────┴───────────────┴───────────────────────╯
 Subtotal:           £19.34  
    Total:           £17.35  
  Savings:   (10.29%) £1.99  

 181µs 956ns (0.000181956s)
```

### Mix and Match Promotions

Mix and Match Bundle promotions define a bundle as a set of required "slots", where each slot must be satisfied by selecting a valid number of qualifying items with specific tags. A bundle only qualifies once all slots are filled.

Typical examples include meal deals (main + drink + snack for a fixed price) and mix-and-match offers (any 3 from a range, with a bundle-level discount applied).

```yaml
meal-deal:
  type: mix_and_match
  name: Meal Deal
  slots:
    - name: main
      tags: [main]
      min: 1
      max: 1
    - name: drink
      tags: [drink]
      min: 1
      max: 1
    - name: snack
      tags: [snack]
      min: 1
      max: 1
  discount:
    type: fixed_total
    amount: 5.00 GBP
```

```bash
cargo run --release --example basket -- -f mix-and-match -n 5
```

```
╭──────┬───────────────────┬───────┬────────────┬──────────────────┬─────────────────┬────────────────╮
│      │ Item              │ Tags  │ Base Price │ Discounted Price │         Savings │ Promotion      │
├──────┼───────────────────┼───────┼────────────┼──────────────────┼─────────────────┼────────────────┤
│ #1   │ Chicken Wrap      │ main  │      £4.00 │            £2.30 │ (42.50%) -£1.70 │ #1   Meal Deal │
├──────┼───────────────────┼───────┼────────────┼──────────────────┼─────────────────┼────────────────┤
│ #2   │ Spring Water      │ drink │      £1.00 │                  │                 │                │
├──────┼───────────────────┼───────┼────────────┼──────────────────┼─────────────────┼────────────────┤
│ #3   │ Apple             │ snack │      £0.80 │                  │                 │                │
├──────┼───────────────────┼───────┼────────────┼──────────────────┼─────────────────┼────────────────┤
│ #4   │ Fruit Smoothie    │ drink │      £2.50 │            £1.44 │ (42.40%) -£1.06 │ #1   Meal Deal │
├──────┼───────────────────┼───────┼────────────┼──────────────────┼─────────────────┼────────────────┤
│ #5   │ Chocolate Brownie │ snack │      £2.20 │            £1.26 │ (42.73%) -£0.94 │ #1   Meal Deal │
╰──────┴───────────────────┴───────┴────────────┴──────────────────┴─────────────────┴────────────────╯
 Subtotal:           £10.50  
    Total:            £6.80  
  Savings:   (35.24%) £3.70  

 89µs 965ns (0.000089965s)
```

### Tiered Threshold Promotions

Tiered Threshold promotions define multiple threshold tiers, where each tier can
use different contribution tags, discount tags, and discount types.

- `lower_threshold` (required) unlocks the tier.
- `upper_threshold` (optional) caps contribution and discountable value per tier
  instance without deactivating it.

The `tiered-threshold` fixture demonstrates all three states as items are added:

- Tier 1 activates.
- Tier 2 overtakes Tier 1.
- Tier 3 overtakes Tier 2.
- Tier 3 stays active but is capped by its upper threshold.

```yaml
tiered-threshold-ladder:
  type: tiered_threshold
  name: Tiered Threshold Ladder
  tiers:
    - lower_threshold:
        monetary: "20.00 GBP"
      contribution_tags: []
      discount_tags: []
      discount:
        type: percent_each_item
        amount: "10%"
    - lower_threshold:
        monetary: "40.00 GBP"
      contribution_tags: []
      discount_tags: []
      discount:
        type: percent_each_item
        amount: "20%"
    - lower_threshold:
        monetary: "60.00 GBP"
      upper_threshold:
        monetary: "80.00 GBP"
      contribution_tags: []
      discount_tags: []
      discount:
        type: percent_each_item
        amount: "30%"
```

```bash
cargo run --release --example basket -- -f tiered-threshold -n 1
```

Increase `-n` to add each item and watch tiers activate, overtake each other,
then cap.

### 1 Item (No Tier Active Yet)

```
╭──────┬────────────────┬────────┬────────────┬──────────────────┬─────────┬───────────╮
│      │ Item           │ Tags   │ Base Price │ Discounted Price │ Savings │ Promotion │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────┼───────────┤
│ #1   │ Ladder Item 01 │ ladder │     £10.00 │                  │         │           │
╰──────┴────────────────┴────────┴────────────┴──────────────────┴─────────┴───────────╯
 Subtotal:          £10.00  
    Total:          £10.00  
  Savings:   (0.00%) £0.00  
```

The basket is below the first lower threshold (£20), so nothing applies.

### 2 Items (Tier 1 Activates)

```bash
cargo run --release --example basket -- -f tiered-threshold -n 2
```

```
╭──────┬────────────────┬────────┬────────────┬──────────────────┬─────────────────┬──────────────────────────────╮
│      │ Item           │ Tags   │ Base Price │ Discounted Price │         Savings │ Promotion                    │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #1   │ Ladder Item 01 │ ladder │     £10.00 │            £9.00 │ (10.00%) -£1.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #2   │ Ladder Item 02 │ ladder │     £10.00 │            £9.00 │ (10.00%) -£1.00 │ #1   Tiered Threshold Ladder │
╰──────┴────────────────┴────────┴────────────┴──────────────────┴─────────────────┴──────────────────────────────╯
 Subtotal:           £20.00  
    Total:           £18.00  
  Savings:   (10.00%) £2.00  
```

Tier 1 unlocks at £20 and applies 10% off.

### 4 Items (Tier 2 Takes Over)

```bash
cargo run --release --example basket -- -f tiered-threshold -n 4
```

```
╭──────┬────────────────┬────────┬────────────┬──────────────────┬─────────────────┬──────────────────────────────╮
│      │ Item           │ Tags   │ Base Price │ Discounted Price │         Savings │ Promotion                    │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #1   │ Ladder Item 01 │ ladder │     £10.00 │            £8.00 │ (20.00%) -£2.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #2   │ Ladder Item 02 │ ladder │     £10.00 │            £8.00 │ (20.00%) -£2.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #3   │ Ladder Item 03 │ ladder │     £10.00 │            £8.00 │ (20.00%) -£2.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #4   │ Ladder Item 04 │ ladder │     £10.00 │            £8.00 │ (20.00%) -£2.00 │ #1   Tiered Threshold Ladder │
╰──────┴────────────────┴────────┴────────────┴──────────────────┴─────────────────┴──────────────────────────────╯
 Subtotal:           £40.00  
    Total:           £32.00  
  Savings:   (20.00%) £8.00  
```

Tier 2 unlocks at £40 and overtakes Tier 1 because it gives a lower total.

### 6 Items (Tier 3 Takes Over)

```bash
cargo run --release --example basket -- -f tiered-threshold -n 6
```

```
╭──────┬────────────────┬────────┬────────────┬──────────────────┬─────────────────┬──────────────────────────────╮
│      │ Item           │ Tags   │ Base Price │ Discounted Price │         Savings │ Promotion                    │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #1   │ Ladder Item 01 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #2   │ Ladder Item 02 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #3   │ Ladder Item 03 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #4   │ Ladder Item 04 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #5   │ Ladder Item 05 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #6   │ Ladder Item 06 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
╰──────┴────────────────┴────────┴────────────┴──────────────────┴─────────────────┴──────────────────────────────╯
 Subtotal:            £60.00  
    Total:            £42.00  
  Savings:   (30.00%) £18.00  
```

Tier 3 unlocks at £60 and overtakes Tier 2.

### 10 Items (Tier 3 Stays Active But Is Capped)

```bash
cargo run --release --example basket -- -f tiered-threshold -n 10
```

```
╭──────┬────────────────┬────────┬────────────┬──────────────────┬─────────────────┬──────────────────────────────╮
│      │ Item           │ Tags   │ Base Price │ Discounted Price │         Savings │ Promotion                    │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #1   │ Ladder Item 01 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #2   │ Ladder Item 02 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #3   │ Ladder Item 03 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #4   │ Ladder Item 04 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #5   │ Ladder Item 05 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #6   │ Ladder Item 06 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #7   │ Ladder Item 07 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #8   │ Ladder Item 08 │ ladder │     £10.00 │            £7.00 │ (30.00%) -£3.00 │ #1   Tiered Threshold Ladder │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #9   │ Ladder Item 09 │ ladder │     £10.00 │                  │                 │                              │
├──────┼────────────────┼────────┼────────────┼──────────────────┼─────────────────┼──────────────────────────────┤
│ #10  │ Ladder Item 10 │ ladder │     £10.00 │                  │                 │                              │
╰──────┴────────────────┴────────┴────────────┴──────────────────┴─────────────────┴──────────────────────────────╯
 Subtotal:           £100.00  
    Total:            £76.00  
  Savings:   (24.00%) £24.00  
```

Tier 3 remains active, but its `upper_threshold.monetary` cap (£80) means only
8 of the 10 £10 items can contribute and be discounted in that tier instance.
The extra items stay full price (and potentially available for other promotions).

## Qualification

By default, `tags: [...]` uses `has_any` behavior (any overlap qualifies). For 
more control, promotions and mix-and-match slots can use a nested `qualification` 
expression:

- `op`: `and` or `or` (defaults to `and`)
- `rules`:
  - `has_all: [...]` item must contain all tags
  - `has_any: [...]` item must contain at least one tag
  - `has_none: [...]` item must contain none of the tags
  - `group: { op, rules }` nested expression

This allows dynamic tags to be composed into rich conditions without 
introducing a separate rule language.

The `qualification` fixture demonstrates nested logic for both direct and
positional promotions:

```yaml
root: all

nodes:
  all:
    promotions: [qualified-snacks-30, qualified-drink-bogof]
    output: pass-through

promotions:
  qualified-snacks-30:
    type: direct_discount
    name: "30% Off Qualified Snacks"
    qualification:
      op: and
      rules:
        - has_any: [snack]
        - group:
            op: or
            rules:
              - has_any: [member]
              - has_any: [student]
        - has_none: [excluded]
    discount:
      type: percentage_off
      amount: 30%

  qualified-drink-bogof:
    type: positional_discount
    name: "BOGOF Qualified Drinks"
    qualification:
      op: and
      rules:
        - has_any: [drink]
        - group:
            op: or
            rules:
              - has_any: [member]
              - has_any: [off-peak]
        - has_none: [hot]
    size: 2
    positions: [1]
    discount:
      type: percentage_off
      amount: 100%
```

```bash
cargo run --release --example basket -- -f qualification
```

```
╭──────┬─────────────────┬──────────┬────────────┬──────────────────┬─────────────────┬───────────────────────────────╮
│      │ Item            │ Tags     │ Base Price │ Discounted Price │         Savings │ Promotion                     │
├──────┼─────────────────┼──────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #1   │ Protein Bar     │ member   │      £2.20 │            £1.54 │ (30.00%) -£0.66 │ #1   30% Off Qualified Snacks │
│      │                 │ peak     │            │                  │                 │                               │
│      │                 │ snack    │            │                  │                 │                               │
├──────┼─────────────────┼──────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #2   │ Granola Pot     │ off-peak │      £1.80 │            £1.26 │ (30.00%) -£0.54 │ #2   30% Off Qualified Snacks │
│      │                 │ snack    │            │                  │                 │                               │
│      │                 │ student  │            │                  │                 │                               │
├──────┼─────────────────┼──────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #3   │ Mixed Nuts      │ peak     │      £1.50 │                  │                 │                               │
│      │                 │ snack    │            │                  │                 │                               │
├──────┼─────────────────┼──────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #4   │ Cola Can        │ drink    │      £1.40 │                  │                 │ #3   BOGOF Qualified Drinks   │
│      │                 │ off-peak │            │                  │                 │                               │
├──────┼─────────────────┼──────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #5   │ Sparkling Water │ drink    │      £1.10 │            £0.00 │   (100%) -£1.10 │ #3   BOGOF Qualified Drinks   │
│      │                 │ member   │            │                  │                 │                               │
├──────┼─────────────────┼──────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #6   │ Hot Latte       │ drink    │      £3.00 │                  │                 │                               │
│      │                 │ hot      │            │                  │                 │                               │
│      │                 │ off-peak │            │                  │                 │                               │
╰──────┴─────────────────┴──────────┴────────────┴──────────────────┴─────────────────┴───────────────────────────────╯
 Subtotal:           £11.00  
    Total:            £8.70  
  Savings:   (20.91%) £2.30  
```

In this run, `30% Off Qualified Snacks` requires `snack AND (member OR student) 
AND NOT excluded`, so `Protein Bar` (`snack + member`) and `Granola Pot` 
(`snack + student`) are discounted, while `Mixed Nuts` is not because it has 
`snack` but neither `member` nor `student`. `BOGOF Qualified Drinks` requires 
`drink AND (member OR off-peak) AND NOT hot`, so `Cola Can` (`drink + off-peak`) 
and `Sparkling Water` (`drink + member`) qualify as the pair, and because it is 
BOGOF, the cheaper of the two is discounted to `£0.00`; `Hot Latte` is excluded 
by `has_none: [hot]`.

## Budgets

Promotions can be configured with two types of budgets:

### Application Budgets

Promotions can be configured with an application count budget, which limits
the number of times a promotion can be applied to the basket. This can be used
to enforce redemption rules such as "once every 30 days" if these are
pre-calculated for the customer.

This can also be used with direct discounts to dynamically make the first `n`
matching items free if they were previously purchased in a bundle by the
customer and are being redeemed over successive orders, or to allow for
promotions like "every 10th item free" that work across multiple orders.

```yaml
snack-bogof:
  type: positional_discount
  name: "Buy One Get One Free Snacks"
  tags: [snack]
  size: 2
  positions: [1]
  discount:
    type: percentage_off
    amount: 100%
  budget:
    applications: 2
```

```bash
cargo run --release --example basket -- -f budget-application
```

```
╭──────┬─────────────────┬───────┬────────────┬──────────────────┬───────────────┬──────────────────────────────────╮
│      │ Item            │ Tags  │ Base Price │ Discounted Price │       Savings │ Promotion                        │
├──────┼─────────────────┼───────┼────────────┼──────────────────┼───────────────┼──────────────────────────────────┤
│ #1   │ Fruit Rollup    │ snack │      £0.80 │                  │               │                                  │
├──────┼─────────────────┼───────┼────────────┼──────────────────┼───────────────┼──────────────────────────────────┤
│ #2   │ Chocolate Bar   │ snack │      £2.50 │                  │               │ #1   Buy One Get One Free Snacks │
├──────┼─────────────────┼───────┼────────────┼──────────────────┼───────────────┼──────────────────────────────────┤
│ #3   │ Sea Salt Crisps │ snack │      £1.20 │                  │               │ #2   Buy One Get One Free Snacks │
├──────┼─────────────────┼───────┼────────────┼──────────────────┼───────────────┼──────────────────────────────────┤
│ #4   │ Fruit Rollup    │ snack │      £0.80 │                  │               │                                  │
├──────┼─────────────────┼───────┼────────────┼──────────────────┼───────────────┼──────────────────────────────────┤
│ #5   │ Chocolate Bar   │ snack │      £2.50 │            £0.00 │ (100%) -£2.50 │ #1   Buy One Get One Free Snacks │
├──────┼─────────────────┼───────┼────────────┼──────────────────┼───────────────┼──────────────────────────────────┤
│ #6   │ Sea Salt Crisps │ snack │      £1.20 │            £0.00 │ (100%) -£1.20 │ #2   Buy One Get One Free Snacks │
╰──────┴─────────────────┴───────┴────────────┴──────────────────┴───────────────┴──────────────────────────────────╯
 Subtotal:            £9.00  
    Total:            £5.30  
  Savings:   (41.11%) £3.70  

 248µs 685ns (0.000248685s)
```

In this example, the BOGOF promotion has a budget of 2 applications. The solver
forms exactly 2 bundles (items 2 & 5 and 3 & 6) using the most expensive items,
leaving the cheaper fruit rollup items (#1 and #4) at full price. It doesn't 
apply promotions first-come-first-served, but instead finds the best combination 
within the budget constraint. Try running with `-n 2` or `-n 4` to see the budget 
constraint activate as items are added.

### Monetary Budgets

Promotions can also be configured with a monetary budget, which limits the total
amount that promotion can discount for the basket. This can be used for
operational limits like "this promotion has £10,000 total spend remaining".

```yaml
clearance-sale:
  type: direct_discount
  name: "50% Off Clearance"
  tags: [clearance]
  discount:
    type: percentage_off
    amount: 50%
  budget:
    monetary: 3.00 GBP
```

```bash
cargo run --release --example basket -- -f budget-monetary
```

```
╭──────┬─────────────────┬───────────┬────────────┬──────────────────┬─────────────────┬────────────────────────╮
│      │ Item            │ Tags      │ Base Price │ Discounted Price │         Savings │ Promotion              │
├──────┼─────────────────┼───────────┼────────────┼──────────────────┼─────────────────┼────────────────────────┤
│ #1   │ A5 Notebook     │ clearance │      £3.49 │            £1.74 │ (50.14%) -£1.75 │ #1   50% Off Clearance │
├──────┼─────────────────┼───────────┼────────────┼──────────────────┼─────────────────┼────────────────────────┤
│ #2   │ Paperback Novel │ clearance │      £6.99 │                  │                 │                        │
├──────┼─────────────────┼───────────┼────────────┼──────────────────┼─────────────────┼────────────────────────┤
│ #3   │ Ballpoint Pen   │ clearance │      £1.99 │            £0.99 │ (50.25%) -£1.00 │ #2   50% Off Clearance │
╰──────┴─────────────────┴───────────┴────────────┴──────────────────┴─────────────────┴────────────────────────╯
 Subtotal:           £12.47  
    Total:            £9.72  
  Savings:   (22.05%) £2.75  

 95µs 887ns (0.000095887s)
```

In this example, all three items qualify for 50% off (with total potential 
savings of £6.23), but the promotion has a £3.00 monetary budget. The solver 
applies the discount to items 1 and 3 (for £2.75 total savings), leaving the 
middle item at full price.

Budget accuracy note:

- Monetary budgets are exact for direct discounts, positional discounts, and 
  cheapest-item discounts.
- For bundle-total discounts (`amount_off_total` / `fixed_total`) in 
  mix-and-match and tiered-threshold promotions, we currently use a 
  conservative estimate when enforcing monetary budgets. This may reject some 
  combinations that would be valid under _exact_ per-bundle accounting. 
  Because of this, these budgets should only be used for operational controls,
  like making sure an alloted "pot" of money is not exceed redeeming the 
  promotions, and not for per-customer account balances (like rewards-wallet 
  tracking).

## Global Optimisation

Baskets are globally optimised for the lowest price given the items added and 
the configured promotions and budgetary constraints. As items are added to the 
basket, promotions may "steal" products from existing applications if doing so 
results in a lower basket price, removing the previous application.

Items may only participate in a single promotion per layer, but can carry multiple 
applications across layers with [stacking](#stacking).

For example, with two configured promotions:

- 15% Off Toiletries (items tagged: `toiletries`)
- 3-for-2 Haircare Mix & Match (items tagged: `haircare`)

The following scenario plays out as items tagged with both `toiletries` and 
`haircare` are added to the basket one by one:

```bash
cargo run --release --example basket -- -f complex -n 1
```

Increase `-n` to add each item to the basket.

### 1 Item

```
╭──────┬───────────────┬────────────┬────────────┬──────────────────┬─────────────────┬─────────────────────────╮
│      │ Item          │ Tags       │ Base Price │ Discounted Price │         Savings │ Promotion               │
├──────┼───────────────┼────────────┼────────────┼──────────────────┼─────────────────┼─────────────────────────┤
│ #1   │ Shampoo 400ml │ haircare   │      £4.50 │            £3.82 │ (15.11%) -£0.68 │ #1   15% Off Toiletries │
│      │               │ toiletries │            │                  │                 │                         │
╰──────┴───────────────┴────────────┴────────────┴──────────────────┴─────────────────┴─────────────────────────╯
 Subtotal:            £4.50  
    Total:            £3.82  
  Savings:   (15.11%) £0.68  

 49µs 587ns (0.000049587s)
```

1 single item has the 15% discount applied.

### 2 Items

```
╭──────┬───────────────────┬────────────┬────────────┬──────────────────┬─────────────────┬─────────────────────────╮
│      │ Item              │ Tags       │ Base Price │ Discounted Price │         Savings │ Promotion               │
├──────┼───────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼─────────────────────────┤
│ #1   │ Shampoo 400ml     │ haircare   │      £4.50 │            £3.82 │ (15.11%) -£0.68 │ #1   15% Off Toiletries │
│      │                   │ toiletries │            │                  │                 │                         │
├──────┼───────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼─────────────────────────┤
│ #2   │ Conditioner 400ml │ haircare   │      £4.00 │            £3.40 │ (15.00%) -£0.60 │ #2   15% Off Toiletries │
│      │                   │ toiletries │            │                  │                 │                         │
╰──────┴───────────────────┴────────────┴────────────┴──────────────────┴─────────────────┴─────────────────────────╯
 Subtotal:            £8.50  
    Total:            £7.22  
  Savings:   (15.06%) £1.28  

 70µs 267ns (0.000070267s)
```

There are still not enough items tagged `haircare` to trigger the 
"3-for-2 Haircare Mix & Match" promotion to apply.

### 3 Items

```
╭──────┬─────────────────────────┬────────────┬────────────┬──────────────────┬─────────────────┬─────────────────────────╮
│      │ Item                    │ Tags       │ Base Price │ Discounted Price │         Savings │ Promotion               │
├──────┼─────────────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼─────────────────────────┤
│ #1   │ Shampoo 400ml           │ haircare   │      £4.50 │            £3.82 │ (15.11%) -£0.68 │ #1   15% Off Toiletries │
│      │                         │ toiletries │            │                  │                 │                         │
├──────┼─────────────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼─────────────────────────┤
│ #2   │ Conditioner 400ml       │ haircare   │      £4.00 │            £3.40 │ (15.00%) -£0.60 │ #2   15% Off Toiletries │
│      │                         │ toiletries │            │                  │                 │                         │
├──────┼─────────────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼─────────────────────────┤
│ #3   │ Travel Shower Gel 100ml │ haircare   │      £1.00 │            £0.85 │ (15.00%) -£0.15 │ #3   15% Off Toiletries │
│      │                         │ toiletries │            │                  │                 │                         │
╰──────┴─────────────────────────┴────────────┴────────────┴──────────────────┴─────────────────┴─────────────────────────╯
 Subtotal:            £9.50  
    Total:            £8.07  
  Savings:   (15.05%) £1.43  

 184µs 460ns (0.00018446s)
```

Now we have 3 `haircare` items, so the 3-for-2 is technically possible, but 
it's a bad deal right now because the "free" item would be the £1.00 shower gel.

Keeping 15% off on all three results in a basket total of £8.07, but applying 
the 3-for-2 promotion results in a basket total of £8.50, so it's better for the 
customer to retain the original 15% discount.

### 4 Items

```
╭──────┬─────────────────────────┬────────────┬────────────┬──────────────────┬─────────────────┬───────────────────────────────────╮
│      │ Item                    │ Tags       │ Base Price │ Discounted Price │         Savings │ Promotion                         │
├──────┼─────────────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────────┤
│ #1   │ Shampoo 400ml           │ haircare   │      £4.50 │                  │                 │ #2   3-for-2 Haircare Mix & Match │
│      │                         │ toiletries │            │                  │                 │                                   │
├──────┼─────────────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────────┤
│ #2   │ Conditioner 400ml       │ haircare   │      £4.00 │                  │                 │ #2   3-for-2 Haircare Mix & Match │
│      │                         │ toiletries │            │                  │                 │                                   │
├──────┼─────────────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────────┤
│ #3   │ Travel Shower Gel 100ml │ haircare   │      £1.00 │            £0.85 │ (15.00%) -£0.15 │ #1   15% Off Toiletries           │
│      │                         │ toiletries │            │                  │                 │                                   │
├──────┼─────────────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────────┤
│ #4   │ Body Wash 500ml         │ haircare   │      £3.00 │            £0.00 │   (100%) -£3.00 │ #2   3-for-2 Haircare Mix & Match │
│      │                         │ toiletries │            │                  │                 │                                   │
╰──────┴─────────────────────────┴────────────┴────────────┴──────────────────┴─────────────────┴───────────────────────────────────╯
 Subtotal:           £12.50  
    Total:            £9.35  
  Savings:   (25.20%) £3.15  

 217µs 785ns (0.000217785s)
```

When the "Body Wash" item is added, the basket reaches a point where the 
previously optimal choice (applying 15% off to every item) is no longer 
globally the cheapest. 

Keeping the flat 15% discount on all items results in a total of £10.62, 
but forming a 3-for-2 haircare bundle allows the engine to group the three most 
expensive eligible items (Shampoo @ £4.50, Conditioner @ £4.00, Body Wash £3.00) 
and make the £3.00 item free, which is worth more than the cumulative 15% saving 
on those three items. The remaining Travel Gel stays on the 15% discount at £0.85, 
giving a new total of £9.35.

### 5 Items

```
╭──────┬─────────────────────────────┬────────────┬────────────┬──────────────────┬─────────────────┬───────────────────────────────────╮
│      │ Item                        │ Tags       │ Base Price │ Discounted Price │         Savings │ Promotion                         │
├──────┼─────────────────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────────┤
│ #1   │ Shampoo 400ml               │ haircare   │      £4.50 │                  │                 │ #3   3-for-2 Haircare Mix & Match │
│      │                             │ toiletries │            │                  │                 │                                   │
├──────┼─────────────────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────────┤
│ #2   │ Conditioner 400ml           │ haircare   │      £4.00 │            £0.00 │   (100%) -£4.00 │ #3   3-for-2 Haircare Mix & Match │
│      │                             │ toiletries │            │                  │                 │                                   │
├──────┼─────────────────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────────┤
│ #3   │ Travel Shower Gel 100ml     │ haircare   │      £1.00 │            £0.85 │ (15.00%) -£0.15 │ #1   15% Off Toiletries           │
│      │                             │ toiletries │            │                  │                 │                                   │
├──────┼─────────────────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────────┤
│ #4   │ Body Wash 500ml             │ haircare   │      £3.00 │            £2.55 │ (15.00%) -£0.45 │ #2   15% Off Toiletries           │
│      │                             │ toiletries │            │                  │                 │                                   │
├──────┼─────────────────────────────┼────────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────────┤
│ #5   │ Deep Repair Hair Mask 250ml │ haircare   │      £6.00 │                  │                 │ #3   3-for-2 Haircare Mix & Match │
│      │                             │ toiletries │            │                  │                 │                                   │
╰──────┴─────────────────────────────┴────────────┴────────────┴──────────────────┴─────────────────┴───────────────────────────────────╯
 Subtotal:           £18.50  
    Total:           £13.90  
  Savings:   (24.86%) £4.60  

 286µs 990ns (0.00028699s)
```

When the "Deep Repair Hair Mask" item is added, the solver re-optimises _again_ 
because the set of eligible `haircare` items has changed, and there is now a 
higher priced item available. 

Because the value of the 3-for-2 promotion depends on which item becomes free, 
the optimal bundle includes the most expensive items, so that the "free" slot 
is maximised. The bundle therefore re-shuffles, to include the Hair Mask (£6.00), 
Shampoo (£4.50), and Conditioner (£4.00), making the £4.00 item free, instead 
of the cheaper Body Wash. 

Body Wash is pushed back out of the bundle, and returns to having just the 15% 
`toiletries` discount.

### Stacking

Promotion stacking is supported via a graph. Promotions are grouped into 
layers, and within each layer promotions compete and each item can be claimed 
by at most one promotion. Items then flow to subsequent layers with updated 
prices, allowing multiple promotions to apply across layers.

A graph can route items based on whether they have participated in any promotions
up to and including that layer. Use `output: split` to send participating and 
non-participating items down different paths, or `output: pass-through` to send 
everything forward together.

```yaml
root: daily-deals

nodes:
  daily-deals:
    promotions: [lunch-deal, drinks-deal]
    output: split
    participating: loyalty-bonus
    non-participating: checkout-coupons

  loyalty-bonus:
    promotions: [loyalty-stacking-bonus]
    output: pass-through

  checkout-coupons:
    promotions: [snack-coupon]
    output: pass-through

promotions:
  lunch-deal:
    type: direct_discount
    name: "Lunch Deal: 25% Off"
    tags: [lunch]
    discount:
      type: percentage_off
      amount: 25%

  drinks-deal:
    type: direct_discount
    name: "Drinks Deal: 20% Off"
    tags: [drink]
    discount:
      type: percentage_off
      amount: 20%

  loyalty-stacking-bonus:
    type: direct_discount
    name: "Loyalty Bonus (on deals)"
    tags: []
    discount:
      type: percentage_off
      amount: 5%

  snack-coupon:
    type: direct_discount
    name: "Coupon: 10% Off Snacks"
    tags: [snack]
    discount:
      type: percentage_off
      amount: 10%
```

```bash
cargo run --release --example basket -- -f layered
```

```
╭──────┬────────────────────┬───────────┬────────────┬──────────────────┬─────────────────┬───────────────────────────────╮
│      │ Item               │ Tags      │ Base Price │ Discounted Price │         Savings │ Promotion                     │
├──────┼────────────────────┼───────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #1   │ Chicken Wrap       │ food      │      £3.50 │                  │                 │                               │
│      │                    │ lunch     │            │                  │                 │                               │
│      │                    │           │      £3.50 │            £2.62 │ (25.14%) -£0.88 │ #1   Lunch Deal: 25% Off      │
│      │                    │           │      £2.62 │            £2.49 │  (4.96%) -£0.13 │ #5   Loyalty Bonus (on deals) │
├──────┼────────────────────┼───────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #2   │ Pasta Salad        │ food      │      £3.00 │                  │                 │                               │
│      │                    │ lunch     │            │                  │                 │                               │
│      │                    │           │      £3.00 │            £2.25 │ (25.00%) -£0.75 │ #2   Lunch Deal: 25% Off      │
│      │                    │           │      £2.25 │            £2.14 │  (4.89%) -£0.11 │ #6   Loyalty Bonus (on deals) │
├──────┼────────────────────┼───────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #3   │ Fresh Orange Juice │ drink     │      £2.00 │                  │                 │                               │
│      │                    │           │      £2.00 │            £1.60 │ (20.00%) -£0.40 │ #3   Drinks Deal: 20% Off     │
│      │                    │           │      £1.60 │            £1.52 │  (5.00%) -£0.08 │ #7   Loyalty Bonus (on deals) │
├──────┼────────────────────┼───────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #4   │ Sparkling Water    │ drink     │      £1.50 │                  │                 │                               │
│      │                    │           │      £1.50 │            £1.20 │ (20.00%) -£0.30 │ #4   Drinks Deal: 20% Off     │
│      │                    │           │      £1.20 │            £1.14 │  (5.00%) -£0.06 │ #8   Loyalty Bonus (on deals) │
├──────┼────────────────────┼───────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #5   │ Morning Newspaper  │ newspaper │      £2.50 │                  │                 │                               │
├──────┼────────────────────┼───────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #6   │ Sea Salt Crisps    │ snack     │      £1.20 │            £1.08 │ (10.00%) -£0.12 │ #9   Coupon: 10% Off Snacks   │
├──────┼────────────────────┼───────────┼────────────┼──────────────────┼─────────────────┼───────────────────────────────┤
│ #7   │ Dark Chocolate Bar │ snack     │      £1.80 │            £1.62 │ (10.00%) -£0.18 │ #10  Coupon: 10% Off Snacks   │
╰──────┴────────────────────┴───────────┴────────────┴──────────────────┴─────────────────┴───────────────────────────────╯
 Subtotal:           £15.50  
    Total:           £12.49  
  Savings:   (19.42%) £3.01  

 107µs 239ns (0.000107239s)
```

## Export ILP Formulation

The `basket` example also supports `-o` to capture the ILP formulation as a
Typst document while solving:

```bash
cargo run --release --example basket -- -f layered -o layered.typ
```

This writes the formulation to:

```text
target/ilp-formulations/layered.typ
```

If you have Typst installed you can then convert this file to PDF:

```bash
typst compile target/ilp-formulations/layered.typ --open
```

There is an ready-made example of of a stacked formulation in `assets/demo.typ` (and the rendered
`assets/demo.pdf`).
