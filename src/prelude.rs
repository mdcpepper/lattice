//! Dante prelude.
//!
//! Convenience exports for common library consumers.

pub use crate::{
    basket::{Basket, BasketError},
    discounts::{DiscountError, SimpleDiscount},
    graph::{GraphError, LayeredSolverResult, OutputMode, PromotionGraph, PromotionGraphBuilder},
    items::{
        Item,
        groups::{ItemGroup, ItemGroupError},
    },
    products::{Product, ProductKey},
    promotions::{
        Promotion, PromotionKey, PromotionMeta, PromotionSlotKey,
        budget::PromotionBudget,
        promotion,
        types::{
            DirectDiscountPromotion, MixAndMatchDiscount, MixAndMatchPromotion, MixAndMatchSlot,
            PositionalDiscountPromotion,
        },
    },
    receipt::{Receipt, ReceiptError},
    solvers::{
        Solver, SolverError, SolverResult,
        ilp::{
            ILPObserver, ILPSolver, NoopObserver,
            renderers::typst::{MultiLayerRenderer, TypstRenderError, TypstRenderer},
        },
    },
    tags::{collection::TagCollection, string::StringTagCollection},
};

pub use crate::promotions::prelude::*;
