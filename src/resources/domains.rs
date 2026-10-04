use crate::client::Lettermint;
use crate::error::Result;
use crate::generated::operations as ops;
use crate::generated::types::{
    DnsVerificationSuccessResponse, DomainData, DomainListData, DomainMutationResponse,
    GetDomainQuery, ListDomainsQuery, ListDomainsResponse, MessageResponse, StoreDomainData,
    UpdateDomainProjectsData,
};
use crate::pagination::Paginator;
use crate::transport::CallOptions;

/// Sending domains. Team token. Returned by [`Lettermint::domains`].
#[derive(Clone)]
pub struct Domains {
    client: Lettermint,
}

super::opaque_debug!(Domains);

impl Domains {
    pub(crate) fn new(client: Lettermint) -> Self {
        Self { client }
    }

    /// Lists domains, one page at a time.
    pub async fn list(&self, query: &ListDomainsQuery) -> Result<ListDomainsResponse> {
        self.client
            .call::<ops::ListDomains>("domains.list", &[], query, None, CallOptions::default())
            .await
    }

    /// Iterates over every domain, following `next_cursor`.
    pub fn iterate(&self, query: &ListDomainsQuery) -> Paginator<DomainListData> {
        self.client
            .paginate::<ops::ListDomains>("domains.iterate", &[], query)
    }

    /// Adds a domain.
    pub async fn create(&self, body: &StoreDomainData) -> Result<DomainData> {
        self.client
            .call::<ops::CreateDomain>(
                "domains.create",
                &[],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// A domain. `query.include` adds `dnsRecords` or `projects`.
    pub async fn retrieve(&self, domain_id: &str, query: &GetDomainQuery) -> Result<DomainData> {
        self.client
            .call::<ops::GetDomain>(
                "domains.retrieve",
                &[domain_id],
                query,
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Deletes a domain.
    pub async fn delete(&self, domain_id: &str) -> Result<MessageResponse> {
        self.client
            .call::<ops::DeleteDomain>(
                "domains.delete",
                &[domain_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Checks every DNS record of the domain.
    pub async fn verify_dns_records(
        &self,
        domain_id: &str,
    ) -> Result<DnsVerificationSuccessResponse> {
        self.client
            .call::<ops::VerifyDomainDnsRecords>(
                "domains.verify_dns_records",
                &[domain_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Checks one DNS record of the domain.
    pub async fn verify_dns_record(
        &self,
        domain_id: &str,
        record_id: &str,
    ) -> Result<MessageResponse> {
        self.client
            .call::<ops::VerifyDomainDnsRecord>(
                "domains.verify_dns_record",
                &[domain_id, record_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Replaces the projects that may send from the domain.
    pub async fn update_projects(
        &self,
        domain_id: &str,
        body: &UpdateDomainProjectsData,
    ) -> Result<DomainMutationResponse> {
        self.client
            .call::<ops::UpdateDomainProjects>(
                "domains.update_projects",
                &[domain_id],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }
}
