//! Cart Handlers

pub(crate) mod create;
pub(crate) mod delete;
pub(crate) mod get;

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use uuid::Uuid;

    use lattice_app::domain::carts::models::Cart;

    pub(super) fn make_cart(uuid: Uuid) -> Cart {
        Cart {
            uuid,
            subtotal: 0,
            total: 0,
            created_at: Timestamp::UNIX_EPOCH,
            updated_at: Timestamp::UNIX_EPOCH,
            deleted_at: None,
        }
    }
}
