//! Promotion marker interface

use ext_php_rs::prelude::*;

/// Marker interface for all PHP promotion configuration objects.
#[php_interface]
#[php(name = "Lattice\\Promotions\\Promotion")]
pub trait Promotion {}
