use crate::client::Lettermint;
use crate::error::Result;
use crate::generated::operations as ops;
use crate::generated::types::{
    GetRouteQuery, InboundDomainVerificationResponse, ListRoutesQuery, ListRoutesResponse,
    MessageResponse, RouteData, RouteListData, RouteMutationResponse, StoreRouteData,
    UpdateRouteData,
};
use crate::pagination::Paginator;
use crate::transport::CallOptions;

/// Routes of a project. Team token. Returned by [`Lettermint::routes`].
#[derive(Clone)]
pub struct Routes {
    client: Lettermint,
}

super::opaque_debug!(Routes);

impl Routes {
    pub(crate) fn new(client: Lettermint) -> Self {
        Self { client }
    }

    /// Lists the routes of a project, one page at a time.
    pub async fn list(
        &self,
        project_id: &str,
        query: &ListRoutesQuery,
    ) -> Result<ListRoutesResponse> {
        self.client
            .call::<ops::ListRoutes>(
                "routes.list",
                &[project_id],
                query,
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Iterates over every route of a project, following `next_cursor`.
    pub fn iterate(&self, project_id: &str, query: &ListRoutesQuery) -> Paginator<RouteListData> {
        self.client
            .paginate::<ops::ListRoutes>("routes.iterate", &[project_id], query)
    }

    /// Creates a route in a project.
    pub async fn create(
        &self,
        project_id: &str,
        body: &StoreRouteData,
    ) -> Result<RouteMutationResponse> {
        self.client
            .call::<ops::CreateRoute>(
                "routes.create",
                &[project_id],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// A route.
    pub async fn retrieve(&self, route_id: &str, query: &GetRouteQuery) -> Result<RouteData> {
        self.client
            .call::<ops::GetRoute>(
                "routes.retrieve",
                &[route_id],
                query,
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Updates a route.
    pub async fn update(
        &self,
        route_id: &str,
        body: &UpdateRouteData,
    ) -> Result<RouteMutationResponse> {
        self.client
            .call::<ops::UpdateRoute>(
                "routes.update",
                &[route_id],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// Deletes a route.
    pub async fn delete(&self, route_id: &str) -> Result<MessageResponse> {
        self.client
            .call::<ops::DeleteRoute>(
                "routes.delete",
                &[route_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Checks the DNS of an inbound route's custom domain.
    pub async fn verify_inbound_domain(
        &self,
        route_id: &str,
    ) -> Result<InboundDomainVerificationResponse> {
        self.client
            .call::<ops::VerifyRouteInboundDomain>(
                "routes.verify_inbound_domain",
                &[route_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }
}
