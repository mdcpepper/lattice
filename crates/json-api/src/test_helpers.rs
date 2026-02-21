//! Test helpers.

use std::sync::Arc;

use salvo::{affix_state::inject, prelude::*};
use uuid::Uuid;

use crate::{
    auth::MockAuthRepository, extensions::*, products::MockProductsRepository, state::State,
    tenants::TenantUuid,
};

pub(crate) const TEST_TENANT_UUID: TenantUuid = TenantUuid::from_uuid(Uuid::nil());

#[salvo::handler]
pub(crate) async fn inject_tenant(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) {
    depot.insert_tenant_uuid(TEST_TENANT_UUID);
    ctrl.call_next(req, depot, res).await;
}

fn strict_auth_mock() -> MockAuthRepository {
    let mut auth = MockAuthRepository::new();

    auth.expect_find_tenant_by_token_hash().never();

    auth
}

fn strict_products_mock() -> MockProductsRepository {
    let mut products = MockProductsRepository::new();

    products.expect_get_products().never();
    products.expect_create_product().never();
    products.expect_update_product().never();
    products.expect_delete_product().never();

    products
}

pub(crate) fn state_with_products(products: MockProductsRepository) -> Arc<State> {
    Arc::new(State::new(Arc::new(products), Arc::new(strict_auth_mock())))
}

pub(crate) fn state_with_auth(auth: MockAuthRepository) -> Arc<State> {
    Arc::new(State::new(Arc::new(strict_products_mock()), Arc::new(auth)))
}

pub(crate) fn products_service(products: MockProductsRepository, route: Router) -> Service {
    Service::new(
        Router::new()
            .hoop(inject(state_with_products(products)))
            .hoop(inject_tenant)
            .push(route),
    )
}
