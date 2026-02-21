//! Product Handlers

pub(crate) mod create;
pub(crate) mod delete;
pub(crate) mod index;
pub(crate) mod update;

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use lattice_app::products::models::Product;
    use uuid::Uuid;

    pub(super) fn make_product(uuid: Uuid) -> Product {
        Product {
            uuid,
            price: 100,
            created_at: Timestamp::UNIX_EPOCH,
            updated_at: Timestamp::UNIX_EPOCH,
            deleted_at: None,
        }
    }
}
