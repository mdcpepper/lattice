# Dante

![Dante](assets/dante.png)
[![Build & Test](https://github.com/mdcpepper/dante/actions/workflows/ci.yml/badge.svg)](https://github.com/mdcpepper/dante/actions/workflows/ci.yml)

Dante is a high-performance, general-purpose pricing, promotion and basket 
optimisation engine written in Rust.

## Promotions

Promotions are rules that select candidate items via tag intersections, and 
apply a discount to them. Promotion applications are treated as a global basket 
optimisation problem and the combination that produces the lowest total basket 
price is chosen.

Each item is either left at full price or claimed by exactly one promotion. 
Promotion types can add their own constraints (e.g. bundled deals) and produce 
per-item applications (including original/final prices and bundle groupings) 
that can be rendered on a receipt. Each promotion type is documented in its own
section below.

### Direct Discount

A simple, direct percentage discount or fixed price override applied to qualifying 
items independently (no bundling).

```
cargo run --release --example direct_discounts
```

In this example "Drink" qualifies for the "20% off" promotion, and "Snack" 
qualifies for both the "20% off" and the "40% off" promotion. Applying the 40% 
promotion to the "Snack" item results in the cheapest total basket price, so 
it is that one that is applied to that item.

```
──────────────────────────────────────────────────────────────────────────────────────────
        Item       Base Price   Discounted Price   Savings          Promotion   Bundle ID 
══════════════════════════════════════════════════════════════════════════════════════════
 #1     Sandwich   £2.99        -                  -                -           -         
──────────────────────────────────────────────────────────────────────────────────────────
 #2     Drink      £1.29        £1.03              £0.26 (20.16%)   20% off     #1        
──────────────────────────────────────────────────────────────────────────────────────────
 #3     Snack      £0.79        £0.47              £0.32 (40.51%)   40% off     #2        
──────────────────────────────────────────────────────────────────────────────────────────

Subtotal: £5.07
Total:    £4.49
Savings:  £0.58 (11.44%)
```
