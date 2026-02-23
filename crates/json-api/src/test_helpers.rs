//! Test helpers.

use std::sync::Arc;

use jiff::Timestamp;
use salvo::{affix_state::inject, prelude::*};
use uuid::Uuid;

use lattice_app::{
    auth::MockAuthService,
    context::AppContext,
    domain::{
        carts::{MockCartsService, models::Cart},
        products::MockProductsService,
        tenants::models::TenantUuid,
    },
};

use crate::{extensions::*, state::State};

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

fn strict_auth_mock() -> MockAuthService {
    let mut auth = MockAuthService::new();

    auth.expect_authenticate_bearer().never();

    auth
}

fn strict_carts_mock() -> MockCartsService {
    let mut carts = MockCartsService::new();

    carts.expect_create_cart().never();
    carts.expect_delete_cart().never();
    carts.expect_get_cart().never();
    carts.expect_add_item().never();
    carts.expect_remove_item().never();

    carts
}

fn strict_products_mock() -> MockProductsService {
    let mut products = MockProductsService::new();

    products.expect_list_products().never();
    products.expect_create_product().never();
    products.expect_update_product().never();
    products.expect_delete_product().never();

    products
}

pub(crate) fn state_with_auth(auth: MockAuthService) -> Arc<State> {
    Arc::new(State::new(AppContext {
        carts: Arc::new(strict_carts_mock()),
        products: Arc::new(strict_products_mock()),
        auth: Arc::new(auth),
    }))
}

pub(crate) fn state_with_carts(carts: MockCartsService) -> Arc<State> {
    Arc::new(State::new(AppContext {
        carts: Arc::new(carts),
        products: Arc::new(strict_products_mock()),
        auth: Arc::new(strict_auth_mock()),
    }))
}

pub(crate) fn state_with_products(products: MockProductsService) -> Arc<State> {
    Arc::new(State::new(AppContext {
        carts: Arc::new(strict_carts_mock()),
        products: Arc::new(products),
        auth: Arc::new(strict_auth_mock()),
    }))
}

pub(crate) fn products_service(products: MockProductsService, route: Router) -> Service {
    Service::new(
        Router::new()
            .hoop(inject(state_with_products(products)))
            .hoop(inject_tenant)
            .push(route),
    )
}

pub(crate) fn carts_service(carts: MockCartsService, route: Router) -> Service {
    Service::new(
        Router::new()
            .hoop(inject(state_with_carts(carts)))
            .hoop(inject_tenant)
            .push(route),
    )
}

pub(crate) fn make_cart(uuid: Uuid) -> Cart {
    Cart {
        uuid,
        subtotal: 0,
        total: 0,
        items: Vec::new(),
        created_at: Timestamp::UNIX_EPOCH,
        updated_at: Timestamp::UNIX_EPOCH,
        deleted_at: None,
    }
}
